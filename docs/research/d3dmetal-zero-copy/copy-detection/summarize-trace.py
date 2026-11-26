#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Match task-only Metal intervals to exact synthetic guest samples."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
from statistics import median
import xml.etree.ElementTree as ET


def table(path, pid):
    root = ET.parse(path)
    elements = {e.attrib['id']: e for e in root.iter() if 'id' in e.attrib}
    schema = [col.findtext('mnemonic') for col in root.findall('.//schema/col')]

    def resolve(element):
        if 'ref' in element.attrib:
            return elements[element.attrib['ref']]
        return element

    rows = []
    for raw in root.findall('.//row'):
        row = dict(zip(schema, map(resolve, raw), strict=True))
        process = row['process'].find('pid')
        if process is None or resolve(process).text != str(pid):
            continue
        rows.append({k: e.text or e.get('fmt') for k, e in row.items()})
    return rows


def key(row):
    return [int(row['cmdbuffer-id']), int(row['encoder-id'])]


def summarize(run, gpu_path, encoding_path):
    receipt = json.loads((run / 'receipt.json').read_text())
    case, = receipt['cases']
    outcome = case['run']
    if outcome['exit'] != 0 or not outcome['backend']['verified']:
        raise ValueError('successful guest and verified backend required')
    pid = int(outcome['backend']['guest_pid'])
    stdout = run / case['directory'] / 'stdout.txt'
    compute = False
    guest = []
    omitted = 0
    for line in stdout.read_text().splitlines():
        name, _, fields = line.partition(' ')
        if name not in ('COMPUTE', 'TRACE_COPY'):
            continue
        row = dict(x.split('=', 1) for x in fields.split())
        actual, expected = map(int, row['matches'].split('/'))
        if actual != expected:
            raise ValueError('guest output mismatch')
        if name == 'COMPUTE':
            compute = True
            if row['mode'] == 'omitted':
                omitted += 1
                continue
        guest.append(row)

    label = 'Compute Command ' if compute else 'Blit Command '
    encoders = [r for r in table(encoding_path, pid)
                if r['event-type'] == 'Encoding' and r['encoder-label'].startswith(label)]
    encoder_keys = {tuple(key(r)) for r in encoders}
    # The GPU channel may be named Compute even for a Blit encoder. Identify
    # commands by the encoder's type and pair of IDs, irrespective of nesting.
    gpu = [r for r in table(gpu_path, pid)
           if r['state'] == 'Active' and tuple(key(r)) in encoder_keys]
    gpu.sort(key=lambda r: int(r['start']))
    encoders.sort(key=lambda r: int(r['start']))
    if not len(guest) == len(gpu) == len(encoders) or not guest:
        raise ValueError(f'Unmatched sample, GPU or encoder counts: {len(guest)}, {len(gpu)}, {len(encoders)}')
    if [key(row) for row in gpu] != [key(row) for row in encoders]:
        raise ValueError('Metal command-buffer and encoder sequence disagrees')
    if len({tuple(key(row)) for row in gpu}) != len(gpu):
        raise ValueError('Duplicate command or encoder pair')

    samples = []
    grouped = defaultdict(list)
    for expected, device, cpu in zip(guest, gpu, encoders, strict=True):
        samples.append(dict(mode=expected.get('mode', 'copy'), iter=int(expected['iter']),
                            workload_payload=int(expected.get('gpu_payload', '0')),
                            read_bytes=int(expected.get('read_bytes', '0')),
                            written_bytes=int(expected.get('written_bytes', '0')),
                            guest_completion_us=float(expected['completion_us']),
                            ids=key(device), channel=device['channel-name'],
                            nested_depth=int(device['event-depth']), encoder_label=cpu['encoder-label'],
                            gpu_start_ns=int(device['start']), gpu_duration_ns=int(device['duration']),
                            metal_encode_ns=int(cpu['duration']), comparison=expected['matches']))
        if samples[-1]['iter'] >= 0:
            grouped[samples[-1]['mode']].append(samples[-1])
    distributions = {}
    for mode, values in grouped.items():
        if len(values) != 12:
            raise ValueError(f'Incorrect measured sample count for {mode}')
        durations = [s['gpu_duration_ns'] / 1000 for s in values]
        mid = median(durations)
        distributions[mode] = dict(n=len(values), median_gpu_us=mid,
            minimum_gpu_us=min(durations), maximum_gpu_us=max(durations),
            mad_gpu_us=median(abs(x - mid) for x in durations),
            payload_GB_s={field: median(s[field] / s['gpu_duration_ns'] for s in values)
                          for field in ('workload_payload', 'read_bytes', 'written_bytes')
                          if values[0][field]})
    return dict(guest_pid=pid, case=case['directory'], matched_intervals=len(gpu),
                omitted_dispatch_guest_samples=omitted, distributions=distributions,
                samples=samples)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('scratch', type=Path)
    parser.add_argument('gpu_intervals', type=Path)
    parser.add_argument('encoders', type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.scratch, args.gpu_intervals, args.encoders), indent=2))


if __name__ == '__main__':
    main()
