#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Summarize raw measured iterations without pooling independent sessions."""
import argparse
import json
import statistics
from pathlib import Path


def distribution(values):
    if not values:
        return None
    ordered = sorted(values)
    n = len(ordered)
    return dict(n=n, min=ordered[0], p10=ordered[(n - 1) // 10],
                median=statistics.median(ordered), p90=ordered[((n - 1) * 9 + 9) // 10],
                max=ordered[-1], mad=statistics.median(
                    abs(x - statistics.median(ordered)) for x in ordered))


def summarize(path):
    receipt = json.loads((path / 'receipt.json').read_text())
    metrics = {}
    rows = []
    for item in receipt['cases']:
        case = item['directory']
        run = item.get('run', {})
        stdout = path / case / 'stdout.txt'
        cases = []
        metrics[case] = dict(exit=run.get('exit'), backend=run.get('backend'),
                             error=run.get('error'), samples=cases)
        if not stdout.is_file():
            continue
        for line in stdout.read_text().splitlines():
            name, _, fields = line.partition(' ')
            if name not in {'COPY', 'CPU', 'COMPUTE', 'EMPTY'}:
                continue
            row = dict(field.split('=', 1) for field in fields.split())
            if int(row['iter']) < 0:
                continue
            keys = ('bytes', 'stride', 'dirty', 'full_write', 'full_read', 'omit', 'mode')
            identity = (case, name, tuple((k, row[k]) for k in keys if k in row))
            rows.append((identity, row))
    groups = {}
    for identity, row in rows:
        groups.setdefault(identity, []).append(row)
    for (case, name, group), samples in groups.items():
        times = {k: distribution([float(s[k]) for s in samples if float(s[k]) >= 0])
                 for k in samples[0] if k.endswith('_us')}
        rates = {}
        for payload, timer, label in [
            ('bytes', 'write_us', 'cpu_write'), ('bytes', 'read_us', 'cpu_read'),
            ('gpu_payload', 'gpu_us', 'gpu_copy'), ('read_bytes', 'gpu_us', 'gpu_read_with_output'),
            ('written_bytes', 'gpu_us', 'gpu_write_with_input'),
            ('gpu_payload', 'completion_us', 'copy_end_to_end'),
            ('read_bytes', 'completion_us', 'shader_read_end_to_end'),
            ('written_bytes', 'completion_us', 'shader_write_end_to_end'),
        ]:
            if payload not in samples[0] or timer not in samples[0]:
                continue
            if label == 'gpu_copy' and group and dict(group).get('full_read') != '1':
                continue
            vals = [int(s[payload]) / float(s[timer]) / 1000 for s in samples
                    if int(s[payload]) > 0 and float(s[timer]) > 0]
            if vals:
                rates[label] = distribution(vals)
        comparisons = [s['matches'].split('/') for s in samples if 'matches' in s]
        metrics[case]['samples'].append(dict(kind=name, factors=dict(group), n=len(samples),
            exact_iterations=sum(a == b for a, b in comparisons) if comparisons else None,
            timestamp_invalid=sum(float(s['gpu_us']) <= 0 for s in samples) if 'gpu_us' in samples[0] else None,
            microseconds=times, decimal_GB_s=rates))
    return dict(directory=path.name, group=receipt['group'], seed=receipt['seed'],
                stop_reason=receipt.get('stop_reason'), cases=metrics)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', nargs='+', type=Path)
    args = parser.parse_args()
    print(json.dumps([summarize(path) for path in args.run], indent=2))


if __name__ == '__main__':
    main()
