#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""CPU-only exact B extractor replay and separate XML/join/statistics verification."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys
import xml.etree.ElementTree as ET


def check(ok, detail):
    if not ok:
        raise ValueError(detail)


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def decode(path, expected_pid):
    tree = ET.parse(path).getroot()
    lookup = {}
    for node in tree.iter():
        if 'id' in node.attrib:
            key = node.attrib['id']
            if key in lookup:
                check(ET.tostring(lookup[key]) == ET.tostring(node), 'conflicting XML id')
            lookup[key] = node
    resolved = 0

    def deref(node, trail=()):
        nonlocal resolved
        if 'ref' not in node.attrib:
            return node
        ref = node.attrib['ref']
        check(ref not in trail and ref in lookup, 'missing/cyclic XML reference')
        resolved += 1
        return deref(lookup[ref], trail + (ref,))

    schema = tree.find('.//schema')
    names = [col.findtext('mnemonic') for col in schema.findall('col')]
    rows = []
    duration_formats = 0
    for row in tree.findall('.//row'):
        check(len(row) == len(names), 'XML column count')
        cells = {name: deref(node) for name, node in zip(names, row)}
        pid = int(deref(cells['process'].find('pid')).text)
        check(pid == expected_pid, 'unexpected process in sanitized input')
        values = {name: node.text or node.get('fmt') for name, node in cells.items()}
        # Independently confirm the nanosecond scale against XML human-readable units.
        node = cells['duration'];match = re.fullmatch(r'([\d.]+)\s*(ns|[µμu]s|ms|s)', node.get('fmt', ''))
        if match:
            factors = {'ns': 1, 'us': 1000, 'µs': 1000, 'μs': 1000, 'ms': 1000000, 's': 1000000000}
            factor = factors[match[2]];places = len(match[1].split('.')[1]) if '.' in match[1] else 0
            check(abs(int(node.text) - float(match[1]) * factor) <= 0.51 * factor * 10**(-places), 'duration unit/format mismatch')
            duration_formats += 1
        rows.append(values)
    return rows, resolved, duration_formats


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('b_worktree', type=Path);ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args();args.output.mkdir(parents=True, exist_ok=False)
    root = args.b_worktree / 'docs/research/d3dmetal-zero-copy'
    inputs = root / 'results/2026-11-27-apple-m5-atesahmet0-b-trace-inputs-01'
    manifest = json.loads((inputs / 'receipt.json').read_text())
    parser = root / 'copy-detection/summarize-trace.py'
    check(digest(parser) == manifest['tools']['docs/research/d3dmetal-zero-copy/copy-detection/summarize-trace.py'], 'extractor source digest')
    report = {'scope': 'CPU-only same-source extraction replay plus independent XML/data audit, no GPU capture or execution', 'b_revision': '4ba4926d2f491517c01b1e94fb8539184055dc22', 'extractor_sha256': digest(parser), 'captures': []}
    for capture in manifest['captures']:
        short = capture['short_name'];run = root / 'results' / capture['published_run_dir']
        rec = json.loads((run / 'receipt.json').read_text());case, = rec['cases'];outcome = case['run']
        pid = int(outcome['backend']['guest_pid']);check(pid == capture['guest_pid'] and outcome['exit'] == 0 and outcome['backend']['verified'], 'receipt process identity')
        files = [inputs / short / capture[k]['sanitized_xml'] for k in ['gpu_intervals', 'encoders']]
        for path, key in zip(files, ['gpu_intervals', 'encoders']):
            check(digest(path) == capture[key]['sanitized_xml_sha256'], 'sanitized input hash')
        cmd = [sys.executable, str(parser), str(run), *map(str, files)]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
        (args.output / (short + '.stdout.json')).write_text(result.stdout)
        (args.output / (short + '.stderr.txt')).write_text(result.stderr)
        check(result.returncode == 0, result.stderr)
        actual = json.loads(result.stdout);published = json.loads((run / 'gpu-samples.json').read_text())
        check(actual == published, 'full extraction JSON differs')
        gpu, gpu_refs, gpu_units = decode(files[0], pid);encoders, enc_refs, enc_units = decode(files[1], pid)
        check(len(gpu) == capture['gpu_intervals']['retained_target_rows'] and len(encoders) == capture['encoders']['retained_target_rows'], 'retained row count')
        is_shader = 'COMPUTE ' in (run / case['directory'] / 'stdout.txt').read_text()
        prefix = 'Compute Command ' if is_shader else 'Blit Command '
        key = lambda r: (int(r['cmdbuffer-id']), int(r['encoder-id']))
        selected = [r for r in encoders if r['event-type'] == 'Encoding' and r['encoder-label'].startswith(prefix)]
        selected.sort(key=lambda r: int(r['start']));ids = [key(r) for r in selected]
        check(len(set(ids)) == len(ids), 'duplicate encoder pair')
        active = {key(r): r for r in gpu if r['state'] == 'Active' and key(r) in set(ids)}
        check(len(active) == len(ids) == len(actual['samples']), 'independent join count')
        check(ids == [key(r) for r in sorted(active.values(), key=lambda r: int(r['start']))], 'GPU/encoder chronological order')
        for item, row in zip(actual['samples'], selected):
            interval = active[key(row)]
            check(tuple(item['ids']) == key(row) and item['encoder_label'] == row['encoder-label'], 'sample identity')
            check(item['gpu_start_ns'] == int(interval['start']) and item['gpu_duration_ns'] == int(interval['duration']) > 0 and item['metal_encode_ns'] == int(row['duration']), 'sample timing')
            check(item['read_bytes'] in [0, 1024, 4096, 4194304] and item['written_bytes'] in [0, 1024, 4096, 262144, 4194304], 'shader denominator')
            if not is_shader:check(item['workload_payload'] == 67108864, 'copy denominator')
        for mode, stats in actual['distributions'].items():
            measured = [v for v in actual['samples'] if v['mode'] == mode and v['iter'] >= 0]
            check(len(measured) == stats['n'] == 12, 'measured sample count')
            times = [v['gpu_duration_ns'] / 1000 for v in measured];mid = statistics.median(times)
            check(mid == stats['median_gpu_us'] and min(times) == stats['minimum_gpu_us'] and max(times) == stats['maximum_gpu_us'], 'median/min/max')
            check(statistics.median(abs(t-mid) for t in times) == stats['mad_gpu_us'], 'MAD')
            for field, value in stats['payload_GB_s'].items():check(statistics.median(v[field] / v['gpu_duration_ns'] for v in measured) == value, 'payload rate')
        report['captures'].append({'name': short, 'pid': pid, 'command': cmd, 'exit': result.returncode, 'exact_json_match': True, 'input_sha256': [digest(p) for p in files], 'published_output_sha256': digest(run / 'gpu-samples.json'), 'selected_intervals': len(ids), 'excluded_encoder_rows': len(encoders)-len(selected), 'resolved_references': gpu_refs+enc_refs, 'duration_format_checks': gpu_units+enc_units, 'outliers_and_warmups_preserved': True})
    check(sum(c['selected_intervals'] for c in report['captures']) == 345, 'total interval count')
    (args.output / 'receipt.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':main()
