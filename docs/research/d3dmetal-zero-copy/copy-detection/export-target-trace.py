#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Export minimal target-PID-only xctrace XML tables preserving referenced XML nodes."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def export_target_table(xml_path: Path, target_pid: str, out_path: Path) -> dict:
    tree = ET.parse(xml_path)
    root = tree.getroot()
    elements = {e.attrib['id']: e for e in root.iter() if 'id' in e.attrib}
    schema = root.find('.//schema')
    if schema is None:
        raise ValueError(f'missing schema in {xml_path}')
    columns = [col.findtext('mnemonic') for col in schema.findall('col')]

    def resolve(element):
        return elements[element.attrib['ref']] if 'ref' in element.attrib else element

    all_rows = root.findall('.//row')
    retained_rows = []
    for raw in all_rows:
        mapped = dict(zip(columns, map(resolve, raw), strict=True))
        pid_node = mapped['process'].find('pid')
        if pid_node is not None and resolve(pid_node).text == target_pid:
            retained_rows.append(copy.deepcopy(raw))

    defined_ids = {
        e.attrib['id'] for row in retained_rows for e in row.iter() if 'id' in e.attrib
    }
    needed_ids = []
    needed_set = set()
    queue = [
        e.attrib['ref'] for row in retained_rows for e in row.iter() if 'ref' in e.attrib
    ]
    while queue:
        ref_id = queue.pop(0)
        if ref_id in defined_ids or ref_id in needed_set:
            continue
        needed_set.add(ref_id)
        needed_ids.append(ref_id)
        target = elements[ref_id]
        for child in target.iter():
            if 'id' in child.attrib and child is not target:
                defined_ids.add(child.attrib['id'])
            if 'ref' in child.attrib:
                queue.append(child.attrib['ref'])

    sanitized_root = ET.Element(root.tag, root.attrib)
    node = ET.SubElement(sanitized_root, 'node')
    node.append(copy.deepcopy(schema))
    if needed_ids:
        defs = ET.SubElement(node, 'referenced-nodes')
        for ref_id in needed_ids:
            defs.append(copy.deepcopy(elements[ref_id]))
    for row in retained_rows:
        node.append(row)

    ET.indent(sanitized_root, space='  ')
    payload = ET.tostring(sanitized_root, encoding='utf-8', xml_declaration=True) + b'\n'
    text = payload.decode('utf-8')
    home = str(Path.home())
    if home in text:
        raise ValueError(f'personal path present in sanitized export for {xml_path}')
    check_root = ET.fromstring(payload)
    pids = {e.text for e in check_root.findall('.//pid') if e.text}
    if not pids <= {target_pid}:
        raise ValueError(f'unexpected PIDs in sanitized export: {pids}')
    processes = sorted(
        {e.attrib.get('fmt', '') for e in check_root.findall('.//process') if 'fmt' in e.attrib}
    )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(payload)
    return {
        'raw_xml_sha256': sha(xml_path),
        'raw_xml_bytes': xml_path.stat().st_size,
        'raw_total_rows': len(all_rows),
        'sanitized_xml': out_path.name,
        'sanitized_xml_sha256': sha(out_path),
        'sanitized_xml_bytes': len(payload),
        'retained_target_rows': len(retained_rows),
        'retained_referenced_nodes': len(needed_ids),
        'retained_processes': processes,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--raw-gpu-intervals', type=Path, required=True)
    parser.add_argument('--raw-encoders', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()

    receipt = json.loads(args.receipt.read_text())
    case, = receipt['cases']
    outcome = case['run']
    if outcome['exit'] != 0 or not outcome['backend']['verified']:
        raise ValueError('verified completed guest case required')
    pid = str(outcome['backend']['guest_pid'])

    summary = {
        'guest_pid': int(pid),
        'case': case['directory'],
        'encoders': export_target_table(
            args.raw_encoders, pid, args.output_dir / 'target-metal-encoders.xml'
        ),
        'gpu_intervals': export_target_table(
            args.raw_gpu_intervals, pid, args.output_dir / 'target-gpu-intervals.xml'
        ),
    }
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
