#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""CPU-only real Python child-process tests of B's run_case finalization; no Wine/GPU."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('b_worktree',type=Path);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args();a.output.mkdir(exist_ok=False)
    child=a.output/'child.py';child.write_text("import os,sys\nprint('CHILD_PID='+str(os.getpid()),flush=True)\nprint('BACKEND_READY',flush=True)\nline=sys.stdin.buffer.read(1)\nif not line:sys.stderr.write('END failure: backend receipt handshake\\r\\n');sys.stderr.flush()\nelse:print('END complete',flush=True)\n")
    parent=a.output/'parent.py';parent.write_text("import subprocess,sys\np=subprocess.Popen([sys.executable,sys.argv[1]],stdin=sys.stdin,stdout=sys.stdout,stderr=sys.stderr,start_new_session=True)\nsys.exit(p.wait())\n")
    old=a.output/'source/docs/research/d3dmetal-zero-copy/copy-detection/run.py';old.parent.mkdir(parents=True);old.write_bytes(subprocess.check_output(['git','-C',str(a.b_worktree),'show','a40228747470f0d184375e451b0173769c90aef5:docs/research/d3dmetal-zero-copy/copy-detection/run.py']))
    new=a.b_worktree/'docs/research/d3dmetal-zero-copy/copy-detection/run.py'
    results=[]
    for tag,path,allow,wait_rc in [('old-stop',old,False,0),('new-stop',new,False,0),('new-normal',new,True,0),('new-wait-timeout',new,True,124)]:
        spec=importlib.util.spec_from_file_location(tag,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
        case=a.output/tag
        def backend(*_):
            content=(case/'stdout.txt').read_text();pid=content.split('CHILD_PID=')[1].splitlines()[0]
            return {'verified':True,'guest_pid':pid,'scope':'actual Python fixture PID; synthetic backend identification, no graphics'}
        m.backend=backend;m.pressure=lambda *_:{'allowed':allow,'scope':'injected process-test condition; not a bypassed GPU pressure guard'}
        calls=[]
        def capture(cmd,*_,**__):
            argv=list(map(str,cmd));rc=wait_rc if '-w' in argv else 0;calls.append({'command':argv,'exit':rc});return {'command':argv,'exit':rc,'stdout':'','stderr':''}
        m.capture=capture
        if tag!='old-stop':
            m.after_guest=lambda directory,result:(directory/'wrapper-final.txt').write_text('completed before sealing\n')
            private=a.output/(tag+'-private');private.mkdir()
        env={k:os.environ[k] for k in ['PATH','HOME','USER','LANG'] if k in os.environ}
        command=[sys.executable,str(parent),str(child)]
        kwargs={} if tag=='old-stop' else {'private':private}
        # has_gpu enables the marker/identity orchestration only; command is Python, never Wine/D3D12.
        result=m.run_case(command,parent,'copy',True,case,env,1,**kwargs)
        deadline=time.monotonic()+2
        if tag=='old-stop':
            while time.monotonic()<deadline and not (case/'stderr.txt').read_bytes():time.sleep(.01)
        actual=hashlib.sha256((case/'stderr.txt').read_bytes()).hexdigest();same=actual==result['output_hashes']['stderr.txt']
        if tag=='old-stop':assert not same,'old late-write race was not reproduced'
        else:assert same,'corrected subprocess output changed after sealing'
        if tag=='new-normal':assert result['exit']==0 and 'error' not in result
        if tag=='new-wait-timeout':assert result['exit']==0 and 'error' not in result and any(c['exit']==124 for c in calls)
        results.append({'case':tag,'exit':result['exit'],'error':result.get('error'),'stderr_recorded_sha256':result['output_hashes']['stderr.txt'],'stderr_actual_sha256':actual,'hash_matches':same,'simulated_wineserver_calls':calls,'normal_cleanup_failure_not_propagated':tag=='new-wait-timeout','fixture_only_no_wine_gpu':True})
    record={'scope':'real controlled Python subprocesses; Wine cleanup is simulated; not a GPU or real wineserver reproduction','b_revision':'4ba4926d2f491517c01b1e94fb8539184055dc22','old_source_sha256':hashlib.sha256(old.read_bytes()).hexdigest(),'new_source_sha256':hashlib.sha256(new.read_bytes()).hexdigest(),'results':results,'remaining_issue':'normal wineserver -w result is discarded; error cleanup return codes are retained but not enforced. Forced cleanup failure cannot certify output finality.'}
    (a.output/'receipt.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record,indent=2))


if __name__=='__main__':main()
