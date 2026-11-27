#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check packed GPU readback against explicit endpoint palettes; no C++ expectation code imported."""
import argparse
import hashlib
import json
from pathlib import Path
import re


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('scratch',type=Path);ap.add_argument('--output',required=True,type=Path);args=ap.parse_args()
    r=json.loads((args.scratch/'receipt.json').read_text());result={'scope':'separate author-written pixel oracle, not independent contributor acceptance','cases':[]}
    if not r['cases'] or r.get('stop_reason') or r.get('not_run'):
        raise ValueError('incomplete or stopped run cannot pass the readback oracle')
    for c in r['cases']:
        omit=c['args'][0]=='omit';variant=0 if omit else int(c['args'][0])
        assert c['exit']==(1 if omit else 0) and not c['timed_out']
        palette=([(255,0,0,255),(0,255,0,255),(255,0,255,255),(0,255,255,255)] if variant%2==0 else [(0,0,255,255),(0,255,0,255),(255,0,255,255),(255,255,0,255)])
        files=sorted((args.scratch/c['name']).glob('readback_*.rgba'));assert len(files)==(1 if omit else 7)
        rows=[]
        for f in files:
            match=re.fullmatch(r'readback_(wb|wc|default)_m([012])_p([01])\.rgba',f.name);assert match
            mode,phase=int(match[2]),int(match[3]);visible=mode==0 or phase==1;raw=f.read_bytes();assert len(raw)==32*16*4
            matched=0
            for y in range(16):
                for x in range(32):
                    expected=bytes(palette[(2 if y>=8 else 0)+(1 if x>=16 else 0)]) if visible else bytes(4)
                    matched+=raw[(y*32+x)*4:(y*32+x+1)*4]==expected
            if omit:assert matched==0,'omitted draw was not detected across all expected pixels'
            else:assert matched==512,(f,matched)
            rows.append({'file':f.name,'matched':matched,'pixels':512,'sha256':hashlib.sha256(raw).hexdigest(),'visible_expected':visible})
        result['cases'].append({'name':c['name'],'negative_control':omit,'frames':rows})
    args.output.write_text(json.dumps(result,indent=2)+'\n');print('checked',sum(len(c['frames']) for c in result['cases']),'raw GPU readback frames')

if __name__=='__main__':main()
