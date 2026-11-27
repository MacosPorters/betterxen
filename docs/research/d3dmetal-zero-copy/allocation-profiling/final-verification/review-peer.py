#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Inspect published peer receipts, fault logs and image-hash expectations without GPU work."""
import argparse
import hashlib
import json
from pathlib import Path
import re


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('b_worktree',type=Path);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
    rs=a.b_worktree/'docs/research/d3dmetal-zero-copy/results';prefix='2026-11-27-apple-m5-atesahmet0-'
    record={'b_revision':'4ba4926d2f491517c01b1e94fb8539184055dc22','scope':'published source/data inspection, no GPU or Wine launch'}
    base=rs/(prefix+'b-baseline-runner-01');d=json.loads((base/'receipt.json').read_text())
    assert hashlib.sha256((base/'probe.cpp').read_bytes()).hexdigest()=='33e90bfee89e812f48990613cd74ced56473e5c3fc5d973f2ba1116090646a0c'
    assert d['run_exit_code']==0 and not d['timed_out']
    record['baseline']={'source_hash_matches':True,'exit':d['run_exit_code'],'timed_out':False,'backend_scope':'separate receipt-only process supplies lsof evidence, not the full benchmark PID'}
    fault=rs/(prefix+'b-a-fence-fault-01');cases=json.loads((fault/'results.json').read_text());record['fault_cases']=[]
    for name,c in cases.items():
        text=(fault/name/'stdout.txt').read_text();assert text==c['stdout'] and 'UNSAFE_UNWIND' not in text
        assert c['exit']==(0 if name=='positive_gate' else 2)
        record['fault_cases'].append({'name':name,'exit':c['exit'],'stdout_matches':True,'canary_output_absent':True})
    record['fault_scope']='positive real queued copy; Signal failure dispatcher called directly; timeout on empty unsignaled fence; no actual Signal HRESULT failure or arbitrary multithreaded teardown injection'
    record['graphics_hash_expectations']=[]
    for group in ['b-a-render-01','b-a-render-omit-01']:
        root=rs/(prefix+group)
        if (root/'rgba-hashes.json').exists():manifest=json.loads((root/'rgba-hashes.json').read_text())
        else:
            omitted=json.loads((root/'archive.json').read_text())['omitted_local_artifacts_sha256']
            manifest={p:{'bytes':2048,'sha256':h} for p,h in omitted.items() if p.endswith('.rgba')}
        for path,entry in manifest.items():
            match=re.fullmatch(r'(render-s[01]|render-omitted-draw-control)/readback_(wb|wc|default)_m([012])_p([01])\.rgba',path);assert match,path
            variant=1 if match[1]=='render-s1' else 0
            visible=(int(match[3])==0 or int(match[4])==1) and match[1]!='render-omitted-draw-control'
            palettes=[[(255,0,0,255),(0,255,0,255),(255,0,255,255),(0,255,255,255)],[(0,0,255,255),(0,255,0,255),(255,0,255,255),(255,255,0,255)]]
            raw=bytes(v for y in range(16) for x in range(32) for v in (palettes[variant][(2 if y>=8 else 0)+(1 if x>=16 else 0)] if visible else (0,0,0,0)))
            assert len(raw)==entry['bytes']==2048 and hashlib.sha256(raw).hexdigest()==entry['sha256']
            record['graphics_hash_expectations'].append({'group':group,'path':path,'expected_sha_matches_reported':True,'raw_rgba_published':(root/path).exists()})
    record['graphics_stop']='render-s1 completed both depth passes; last marker STAGE stencil, Urgent stop; repeat incomplete'
    record['forensic_delta_ns']=1795730540932343519-1795730540928687009;assert record['forensic_delta_ns']==3656510
    record['forensic_timestamp_provenance']='published B stat table, not an independently retrieved APFS transcript; separate A process fixture reproduces the hash race'
    record['missing_artifacts']=['independent B bitmap checker and M5 logical rerun raw','raw RGBA bytes (hash-only verification available)','raw stat transcript / corrected forced-stop receipt']
    # The paired allocator run is a subset, not a six-case M5 reproduction.
    allocator=rs/(prefix+'b-a-d3d12ma-01');r=json.loads((allocator/'receipt.json').read_text());assert len(r['cases'])==2
    record['allocator_subset']=[]
    for c in r['cases']:
        assert c['exit']==0 and not c['timed_out'];text=(allocator/c['name']/'stdout.txt').read_text()
        alloc=[line for line in text.splitlines() if line.startswith('REFERENCE_ALLOC ')]
        stats=[line for line in text.splitlines() if line.startswith('ALLOCATOR_STATS ')]
        reads=[line for line in text.splitlines() if line.startswith('REFERENCE_VERIFY ')]
        assert len(alloc)==432 and len(stats)==36 and len(reads)==24
        assert all('matched=1152/1152' in line for line in reads)
        assert 'POOL_BOUND_CONTROL requested_bytes=33554436 HRESULT=0x8007000E rejected=1' in text
        record['allocator_subset'].append({'case':c['name'],'requests':len(alloc),'stats':len(stats),'readbacks':len(reads),'bound_rejected':True})
    a.output.write_text(json.dumps(record,indent=2)+'\n');print('Peer evidence inspected:',len(cases),'fault cases,',len(record['graphics_hash_expectations']),'hash expectations,',len(record['allocator_subset']),'allocator cases')


if __name__=='__main__':main()
