#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run bounded logical allocation traces and a separate interval oracle; no GPU work."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--scratch',required=True,type=Path);ap.add_argument('--seeds',default='473,474,475');args=ap.parse_args()
    src=Path(__file__).resolve().parent;out=args.scratch.expanduser().resolve()
    if out.exists():ap.error('scratch must be new')
    if src==out or src in out.parents:ap.error('scratch cannot be inside source directory')
    seeds=[int(x) for x in args.seeds.split(',')]
    if not seeds or len(seeds)>3:ap.error('one to three bounded traces')
    out.mkdir(parents=True);(out/'source').mkdir();exe=out/'trace'
    receipt=dict(protocol='A-logical-fragmentation-1',utc=datetime.now(timezone.utc).isoformat(),scope='logical spans only; no 32 MiB physical allocation, no GPU/driver-fragmentation claim',source_hashes={},tools=[],cases=[])
    for p in src.iterdir():
        if p.is_file() and p.suffix in ['.cpp','.py']:
            receipt['source_hashes'][p.name]=hashlib.sha256(p.read_bytes()).hexdigest();shutil.copy2(p,out/'source'/p.name)
    def capture(cmd,timeout=20):
        p=subprocess.run([str(x) for x in cmd],capture_output=True,timeout=timeout)
        return dict(command=[str(x) for x in cmd],exit=p.returncode,stdout=p.stdout.decode(errors='replace'),stderr=p.stderr.decode(errors='replace'))
    try:
        build=capture(['xcrun','clang++','-std=c++20','-O2','-arch','arm64','-Wall','-Wextra','-Werror',src/'trace.cpp','-o',exe],60);receipt['build']=build
        if build['exit']:raise RuntimeError('build failed')
        receipt['executable_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest()
        for cmd in [['file',exe],['lipo','-archs',exe],['otool','-L',exe],['xcrun','clang++','--version'],['sw_vers'],['sysctl','-n','kern.memorystatus_vm_pressure_level']]:receipt['tools'].append(capture(cmd))
        for seed in seeds:
            cmd=[str(exe),str(seed)];p=subprocess.run(cmd,capture_output=True,timeout=15)
            stdout=out/f'seed-{seed}-stdout.jsonl';stderr=out/f'seed-{seed}-stderr.txt';stdout.write_bytes(p.stdout);stderr.write_bytes(p.stderr)
            oracle=capture(['python3',src/'oracle.py',stdout,'--output',out/f'seed-{seed}-oracle.json'])
            receipt['cases'].append(dict(seed=seed,command=cmd,exit=p.returncode,oracle=oracle))
            if p.returncode or oracle['exit']:raise RuntimeError('trace/oracle failure')
        # An intentionally corrupted COPY is a negative control for the independent oracle.
        bad=out/'corrupted-control.jsonl';rows=(out/f'seed-{seeds[0]}-stdout.jsonl').read_text().splitlines()
        for i,line in enumerate(rows):
            row=json.loads(line)
            if row['op']=='stats':
                row['largest_free']+=65536;rows[i]=json.dumps(row);break
        bad.write_text('\n'.join(rows)+'\n')
        control=capture(['python3',src/'oracle.py',bad,'--output',out/'should-not-exist.json']);receipt['corrupted_log_control']=control
        if control['exit']==0 or 'AssertionError' not in control['stderr']:raise RuntimeError('oracle did not reject corrupted statistic')
        print('logical traces checked:',len(seeds),'; corrupted-statistic control rejected')
    finally:
        receipt['output_hashes']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in out.iterdir() if p.is_file() and p!=exe}
        (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')

if __name__=='__main__':main()
