#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent A recalculation of B's published records; no guest or B code execution."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import statistics


def require(ok, message):
    if not ok:
        raise ValueError(message)


def fields(line):
    return dict(re.findall(r'(\w+)=([^\s]+)', line))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('b_research_root', type=Path)
    ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args()
    root = args.b_research_root
    results = root / 'results'
    summary = dict(scope='A source/raw-data review; no M4 hardware reproduction or physical-mechanism acceptance', source_snapshots=0, watch_phases=0, sweep_measured=0, compute_measured=0, compute_endpoint_pairs=0, traces=[])
    for receipt in sorted(results.glob('*/receipt.json')):
        r = json.loads(receipt.read_text())
        for relative, sha in r.get('sources', {}).items():
            source = receipt.parent / 'source' / relative
            require(source.is_file() and hashlib.sha256(source.read_bytes()).hexdigest() == sha, ('source hash', source))
            summary['source_snapshots'] += 1
        if r.get('group') not in ['small', 'sweep', 'compute']:
            continue
        for case in r['cases']:
            run = case.get('run', {})
            if run.get('exit') != 0 or run.get('error'):
                continue
            directory = receipt.parent / case['directory']
            text = (directory / 'stdout.txt').read_text()
            for line in text.splitlines():
                f = fields(line)
                if line.startswith('WATCH phase='):
                    expected = {'fresh': [], 'only_second_guest_page': [4096], 'query_without_reset': [4096], 'second_and_third': [4096, 8192], 'after_query_reset': [], 'same_page_rearmed': [4096], 'host_boundary': [12288, 16384], 'after_explicit_reset': [], 'after_read_only': [], 'same_value_store_after_reset': [16384], 'final_empty': []}[f['phase']]
                    actual_match = re.search(r' actual=(.*?) expected=', line)
                    actual = [int(v) for v in actual_match[1].split(',') if v]
                    require(f['rc'] == '0' and f['granularity'] == '4096' and int(f['count']) == len(expected) and actual == expected, line)
                    summary['watch_phases'] += 1
                elif line.startswith('COPY '):
                    a, b = map(int, f['matches'].split('/'))
                    require(a == b and f['expected'] == f['actual'], line)
                    if int(f['iter']) >= 0 and receipt.parent.name.endswith(('b-sweep-01', 'b-sweep-02')):
                        summary['sweep_measured'] += 1
                elif line.startswith('COMPUTE '):
                    outputs = int(f['matches'].split('/')[1])
                    mode = f['mode']
                    nloads, stride = {'both': (1, 1), 'full_read': (16, 1), 'sparse_read_4k': (1, 1024), 'sparse_read_16k': (1, 4096), 'write': (0, 1), 'omitted': (1, 1)}[mode]
                    seed = int(f['seed'])
                    expected = []
                    for j in [0, outputs - 1]:
                        total = seed + 37 * j
                        for n in range(nloads):
                            k = stride * (j + n * outputs)
                            value = ((k ^ ((seed * 0x01010301) & 0xffffffff)) + 17 * k) & 0xffffffff
                            total += value
                        expected.append(0x6ba8fa03 if mode == 'omitted' else ((total & 0xffffffff) ^ 0xa5c35a3c))
                    actual = [int(v, 16) for v in f['actual'].split(',')]
                    require(actual == expected and int(f['matches'].split('/')[0]) == outputs, line)
                    summary['compute_endpoint_pairs'] += 1
                    if int(f['iter']) >= 0 and receipt.parent.name.endswith(('b-compute-01', 'b-compute-02')):
                        summary['compute_measured'] += 1
    for path in sorted(results.glob('*/gpu-samples.json')):
        data = json.loads(path.read_text())
        require(len(data['samples']) == data['matched_intervals'] and len({tuple(v['ids']) for v in data['samples']}) == len(data['samples']), str(path))
        groups = {}
        for sample in data['samples']:
            require(sample['gpu_duration_ns'] > 0 and len(set(sample['comparison'].split('/'))) == 1, str(path))
            if sample['iter'] >= 0:
                groups.setdefault(sample['mode'], []).append(sample)
        for mode, samples in groups.items():
            require(len(samples) == 12, (path, mode))
            durations = [v['gpu_duration_ns'] / 1000 for v in samples]
            mid = statistics.median(durations)
            reported = data['distributions'][mode]
            require(abs(reported['median_gpu_us'] - mid) < 1e-8, (path, mode, 'median'))
            require(abs(reported['mad_gpu_us'] - statistics.median(abs(v - mid) for v in durations)) < 1e-8, (path, mode, 'mad'))
            for numerator, rate in reported['payload_GB_s'].items():
                derived = statistics.median(v[numerator] / v['gpu_duration_ns'] for v in samples)
                require(abs(derived - rate) < 1e-8, (path, mode, 'rate'))
        summary['traces'].append(dict(path=str(path.relative_to(root)), intervals=len(data['samples']), measured_groups=len(groups)))
    require(summary['sweep_measured'] == 5040 and summary['compute_measured'] == 576, ('incomplete reported uninstrumented profile', summary))
    args.output.write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
