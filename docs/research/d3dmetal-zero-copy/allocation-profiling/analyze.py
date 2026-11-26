#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Recalculate descriptive A-local-2 statistics from raw receipts/logs, without editing them."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import statistics


def distribution(values):
    s=sorted(values)
    if not s: return None
    return dict(n=len(s),median=statistics.median(s),minimum=s[0],maximum=s[-1],
                p10=s[max(0,math.ceil(len(s)*.1)-1)],p90=s[max(0,math.ceil(len(s)*.9)-1)])


def fields(line):
    return dict(re.findall(r'(\w+)=([^\s]+)',line))


def analyze(base,allow_partial=False):
    receipt=json.loads((base/'receipt.json').read_text())
    result=dict(protocol=receipt['protocol'],suite=receipt['suite'],receipt_sha256=hashlib.sha256((base/'receipt.json').read_bytes()).hexdigest(),
                review='author-run; independent acceptance pending',not_run=receipt.get('not_run',[]),cases=[],tool_failures={k:v['exit'] for k,v in receipt['tools'].items() if v['exit']})
    for case in receipt['cases']:
        name=case['name'];root=base/name;raw=(root/'stdout.txt').read_bytes();out=raw.decode()
        if case['exit']!=case.get('expected_exit',0) or case['timed_out']:
            if not allow_partial: raise ValueError('unsatisfied case '+name)
            result['cases'].append(dict(name=name,exit=case['exit'],status='incomplete or failed; not counted as passed',harness_error=case.get('harness_error'),snapshots=len(case['snapshots']),outcomes=[l for l in out.splitlines() if l.startswith(('OUTCOME ','NEGATIVE','END '))]))
            continue
        for output,digest in case.get('outputs',{}).items():
            if hashlib.sha256((root/output).read_bytes()).hexdigest()!=digest:raise ValueError('changed output '+name+'/'+output)
        c=dict(name=name,exit=case['exit'],seconds=case['seconds'],backend=case.get('backend_paths_excerpt',[]))
        if receipt['suite']=='memory':
            stages={}
            for snap in case['snapshots']:
                values=[x['data'] for x in snap['samples'] if x.get('exit')==0 and x.get('data',{}).get('return')==0]
                if len(values)!=3:raise ValueError('missing native accounting samples')
                stage=dict(physical_footprint_bytes=statistics.median(x['phys_footprint'] for x in values),
                           resident_bytes=statistics.median(x['resident_size'] for x in values))
                for tool in ['footprint','vmmap','pressure','swap']:
                    stage[tool+'_exit']=snap[tool]['exit']
                ptr=int(snap['marker'].split('cpu_ptr=')[1],16)
                if ptr:stage.update(cpu_pointer_hex=hex(ptr),cpu_pointer_mod_64KiB=ptr%65536,cpu_pointer_mod_2MiB=ptr%2097152)
                stages[snap['phase']]=stage
            if len(stages)!=10:raise ValueError('incomplete memory checkpoints')
            baseline=stages['baseline']['physical_footprint_bytes']
            for v in stages.values():v['delta_baseline_bytes']=v['physical_footprint_bytes']-baseline
            c.update(args=case['args'],stages=stages)
            if case['args'][0] not in ['cpu','cpu-plain'] and 'checksum_matches=256/256' not in out:raise ValueError('missing full consumption checksum')
        elif receipt['suite']=='pool':
            alloc=[fields(l) for l in out.splitlines() if l.startswith('ALLOC ')]
            frees=[fields(l) for l in out.splitlines() if l.startswith('FREE ')]
            if case.get('expected_exit')==1:
                if 'END failure reason=trace guard/interior mismatch' not in out:raise ValueError('negative control reason')
                c['negative_control']='omitted copy detected'
            else:
                c['allocation_ms']={k:distribution([float(v[k]) for v in alloc if int(v['iteration'])>=0]) for k in ['heap_or_suballoc_ms','placement_ms','map_ms']}
                c['free_ms']={k:distribution([float(v[k]) for v in frees if int(v['iteration'])>=0]) for k in ['resource_ms','backing_or_freelist_ms']}
                c['cycle_ms']=distribution([float(fields(l)['ms']) for l in out.splitlines() if l.startswith('CYCLE ') and int(fields(l)['iteration'])>=0])
                c['fragmentation']=[fields(l) for l in out.splitlines() if l.startswith('FRAGMENT ')]
                checks=[fields(l) for l in out.splitlines() if l.startswith('READBACK ')]
                if len(checks)!=24:raise ValueError('missing trace readback checks')
                for x in checks:
                    a,b=map(int,x['matches'].split('/'))
                    if a!=b:raise ValueError('readback mismatch')
                if 'withheld=1' not in out or 'DELAYED_RETIRE' not in out:raise ValueError('missing delayed retirement')
                if c['cycle_ms']['n']!=10 or c['allocation_ms']['map_ms']['n']!=360:raise ValueError('wrong trace sample count')
                c['readback_checks']=len(checks)
        elif receipt['suite']=='alias':
            c['rounds']=[fields(l) for l in out.splitlines() if l.startswith('BUFFER_INHERIT ')]
            if len(c['rounds'])!=3:raise ValueError('missing inheritance rounds')
            for row in c['rounds']:
                for key in ['source_matches','inherited_matches','disjoint_guard_matches']:
                    if row[key]!='16384/16384':raise ValueError('inheritance mismatch')
            c['addresses']=[fields(l) for l in out.splitlines() if l.startswith('ALIAS_RESOURCE ')]
        else:
            c['outcomes']=[l for l in out.splitlines() if l.startswith(('OUTCOME ','NEGATIVE ','FOOTPRINT '))]
        result['cases'].append(c)
    return result


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('scratch',type=Path);p.add_argument('--allow-partial',action='store_true');p.add_argument('--output',required=True,type=Path);args=p.parse_args()
    result=analyze(args.scratch,args.allow_partial);args.output.write_text(json.dumps(result,indent=2)+'\n');print(result['suite'],len(result['cases']),'cases; tool failures',result['tool_failures'])

if __name__=='__main__':main()
