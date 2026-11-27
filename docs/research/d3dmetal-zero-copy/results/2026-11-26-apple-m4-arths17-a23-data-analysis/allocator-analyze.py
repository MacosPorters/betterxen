#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Re-derive recorded allocator spans/statistics; no upstream allocation algorithm used."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import statistics


def fields(line):
    return dict(re.findall(r'(\w+)=([^\s]+)', line))


def check(condition, detail):
    if not condition:
        raise ValueError(detail)


def analyze(root, raw):
    receipt = json.loads((root / 'receipt.json').read_text())
    check(len(receipt['cases']) == 6 and not receipt.get('stop_reason'), 'incomplete six-case profile')
    result = {'scope': 'author-written data re-derivation, independent acceptance pending', 'cases': []}
    for case in receipt['cases']:
        check(case['exit'] == 0 and not case['timed_out'], case['name'])
        text = (root / case['name'] / 'stdout.txt').read_text()
        live, requests, allocations, frees, cycles = {}, [], [], [], []
        verified = stats = bound = 0
        peak_payload = peak_reserved = peak_waste = 0
        for line in text.splitlines():
            v = fields(line)
            if line.startswith('POOL_BOUND_CONTROL '):
                check(v['rejected'] == '1' and v['HRESULT'] == '0x8007000E', line)
                bound += 1
            elif line.startswith('REFERENCE_ALLOC '):
                slot, size, reserved, offset = (int(v[k]) for k in ['slot', 'bytes', 'allocation_size', 'offset'])
                check(slot not in live and offset % 65536 == 0 and reserved >= size and offset + reserved <= 33554432, line)
                check(all(offset + reserved <= a[0] or a[0] + a[1] <= offset for a in live.values()), 'overlap: ' + line)
                live[slot] = (offset, reserved, size)
                peak_payload = max(peak_payload, sum(a[2] for a in live.values()))
                peak_reserved = max(peak_reserved, sum(a[1] for a in live.values()))
                peak_waste = max(peak_waste, sum(a[1] - a[2] for a in live.values()))
                requests.append(tuple(int(v[k]) for k in ['cycle', 'phase', 'slot', 'bytes']))
                if int(v['cycle']) >= 0:
                    allocations.append(float(v['create_resource_ms']))
            elif line.startswith('REFERENCE_FREE '):
                check(int(v['slot']) in live, line)
                del live[int(v['slot'])]
                if int(v['cycle']) >= 0:
                    frees.append(float(v['release_resource_and_allocation_ms']))
            elif line.startswith('ALLOCATOR_STATS '):
                spans = sorted(live.values())
                cursor, gaps = 0, []
                for start, length, _ in spans:
                    if start > cursor:
                        gaps.append(start - cursor)
                    cursor = start + length
                if cursor < 33554432:
                    gaps.append(33554432 - cursor)
                expected = dict(blocks=1, block_bytes=33554432, allocation_count=len(live), allocation_bytes=sum(a[1] for a in spans), unused_ranges=len(gaps), largest_free=max(gaps, default=0))
                check(all(int(v[k]) == value for k, value in expected.items()), ('statistics', v, expected))
                stats += 1
            elif line.startswith('REFERENCE_VERIFY '):
                actual, total = map(int, v['matched'].split('/'))
                check(actual == total == 24 * 3 * 16 and len(live) == 24, line)
                verified += 1
            elif line.startswith('REFERENCE_CYCLE ') and int(v['cycle']) >= 0:
                cycles.append(float(v['ms']))
        check(not live and len(requests) == 432 and len(allocations) == len(frees) == 360 and len(cycles) == 10 and stats == 36 and verified == 24 and bound == 1 and text.rstrip().endswith('END success'), 'missing events: ' + case['name'])
        seed = int(case['name'].rsplit('-', 1)[1])
        original = (raw / f'pool-pooled-s{seed - 473}' / 'stdout.txt').read_text()
        original_requests = []
        for line in original.splitlines():
            if line.startswith('ALLOC '):
                v = fields(line)
                if int(v['iteration']) >= -2:
                    original_requests.append(tuple(int(v[k]) for k in ['iteration', 'phase', 'slot', 'bytes']))
        check(requests == original_requests, 'request sequence differs from seed-matched raw trace')
        result['cases'].append(dict(name=case['name'], stdout_sha256=hashlib.sha256((root / case['name'] / 'stdout.txt').read_bytes()).hexdigest(), peak_live_payload_bytes=peak_payload, peak_reserved_suballocation_bytes=peak_reserved, peak_internal_rounding_bytes=peak_waste, matched_raw_requests=len(requests), statistics_checked=stats, readbacks_checked=verified, oversized_request_rejected=True, measured_allocations=len(allocations), create_resource_median_ms=statistics.median(allocations), release_median_ms=statistics.median(frees), cycle_median_ms=statistics.median(cycles)))
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('results', type=Path)
    ap.add_argument('--raw-pool-results', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    a = ap.parse_args()
    result = analyze(a.results, a.raw_pool_results)
    a.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
