#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile an external pinned allocator reference; run only within explicit pressure/headroom bounds."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import signal
import subprocess
import threading
from datetime import datetime,timezone


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--external',type=Path,required=True);ap.add_argument('--scratch',type=Path,required=True);ap.add_argument('--bottle',type=Path);ap.add_argument('--build-only',action='store_true');ap.add_argument('--allow-warning',action='store_true');ap.add_argument('--seeds',default='473,474,475');args=ap.parse_args()
    src=Path(__file__).resolve().parent;parent=src.parent;out=args.scratch.expanduser().resolve();up=args.external.expanduser().resolve()
    if out.exists():ap.error('append-only scratch must be new')
    if not args.build_only and args.bottle is None:ap.error('--bottle required for execution')
    seeds=[int(v) for v in args.seeds.split(',')]
    if not seeds or len(seeds)>3:ap.error('one to three bounded paired seeds')
    spec=importlib.util.spec_from_file_location('a_capture',parent/'run.py');helpers=importlib.util.module_from_spec(spec);spec.loader.exec_module(helpers)
    env={k:os.environ[k] for k in ['HOME','USER','LOGNAME','PATH','LANG','LC_ALL'] if k in os.environ};out.mkdir(parents=True,mode=0o700);env.update(TMPDIR=str(out)+'/',ROSETTA_ADVERTISE_AVX='1',CX_GRAPHICS_BACKEND='d3dmetal',WINEDEBUG='-all')
    r=dict(protocol='A-external-allocator-1',utc=datetime.now(timezone.utc).isoformat(),upstream_revision='600949266be004553f74c9ef809384380a9d44cf',upstream_modifications='none',runtime_executed=False,allow_warning=args.allow_warning,cases=[],tools={},sources={},external={})
    def save():(out/'receipt.json').write_text(json.dumps(r,indent=2)+'\n')
    def capture(cmd,timeout=20):return helpers.capture(cmd,env,out,timeout)
    expected={'LICENSE.txt':'3d5a69a6f5695507ecc9c412c783ca60572bf878','D3D12MemAlloc.h':'b685a5cd6188519606efaf039cd88c2faffa261a','D3D12MemAlloc.cpp':'b7ba60a3a4deb0ed466b235b7c5298f91ba5c976'}
    for name,blob in expected.items():
        p=up/name;data=p.read_bytes();actual=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        if actual!=blob:ap.error('external source bytes differ: '+name)
        r['external'][name]=dict(git_blob=actual,sha256=helpers.sha(p))
    snap=out/'source';(snap/'allocator-evaluation').mkdir(parents=True)
    for p,dst in [(src/'probe.cpp',snap/'allocator-evaluation/probe.cpp'),(src/'run.py',snap/'allocator-evaluation/run.py'),(parent/'support.h',snap/'support.h'),(parent/'observe.cpp',snap/'observe.cpp'),(parent/'run.py',snap/'capture-helper.py')]:shutil.copy2(p,dst);r['sources'][p.name if p!=parent/'run.py' else 'capture-helper.py']=helpers.sha(p)
    exe=out/'reference.exe';observer=out/'observe';cx=Path('/Applications/CrossOver.app/Contents/SharedSupport/CrossOver')
    cmds={'build_pe':['x86_64-w64-mingw32-g++','-std=c++20','-O2','-static','-static-libgcc','-static-libstdc++','-Wall','-Wextra','-Werror','-I',up,snap/'allocator-evaluation/probe.cpp',up/'D3D12MemAlloc.cpp','-o',exe,'-lkernel32','-lole32','-ldxguid'],
          'build_observer':['xcrun','clang++','-std=c++20','-O2','-arch','arm64','-Wall','-Wextra','-Werror',snap/'observe.cpp','-o',observer],
          'formats':['file',exe,observer],'observer_arch':['lipo','-archs',observer],'observer_links':['otool','-L',observer],
          'gcc':['x86_64-w64-mingw32-g++','--version'],'macos':['sw_vers'],'chip':['sysctl','-n','machdep.cpu.brand_string'],'ram':['sysctl','-n','hw.memsize'],
          'crossover':[cx/'bin/wine','--version'],'wine':[cx/'bin/wineloader','--version'],'rosetta':['pkgutil','--pkg-info','com.apple.pkg.RosettaUpdateAuto'],
          'imports':['x86_64-w64-mingw32-objdump','-p',exe]}
    for name,cmd in cmds.items():
        r['tools'][name]=capture(cmd,120 if name.startswith('build') else 20);save()
        if name.startswith('build') and r['tools'][name]['exit']:raise RuntimeError('build failed: '+name)
    r['binaries']={p.name:helpers.sha(p) for p in [exe,observer]};resources=cx/'lib64/apple_gptk/external/D3DMetal.framework/Versions/A/Resources';r['framework']={p.name:plistlib.loads(p.read_bytes()) for p in resources.glob('*.plist')};save()
    if args.build_only:print('external pinned allocator compiled; runtime NOT_RUN by --build-only');return 0
    def pressure():
        k=capture(['sysctl','-n','kern.memorystatus_vm_pressure_level']);vm=capture(['vm_stat']);ram=capture(['sysctl','-n','hw.memsize']);percent=capture(['memory_pressure','-Q']);swap=capture(['sysctl','vm.swapusage'])
        page=re.search(r'page size of (\d+) bytes',vm['stdout']);free=re.search(r'Pages free:\s+(\d+)',vm['stdout']);file=re.search(r'File-backed pages:\s+(\d+)',vm['stdout']);pct=re.search(r'free percentage: (\d+)%',percent['stdout'])
        if any(x['exit'] for x in [k,vm,ram,percent,swap]) or not all([page,free,file,pct]) or not k['stdout'].strip().isdigit():return dict(safe=False,reason='pressure measurement unavailable',kernel=k,vmstat=vm,percentage=percent)
        used=int(ram['stdout'])-(int(free[1])+int(file[1]))*int(page[1]);safe=int(k['stdout'])<=(1 if args.allow_warning else 0) and used+512*1024**2<23000000000 and int(pct[1])>=30
        return dict(safe=safe,kernel=k,vmstat=vm,percentage=percent,swap=swap,conservative_non_file_cache_occupied_bytes=used,ceiling_bytes=23000000000)
    pre=pressure();r['preflight']=pre;save()
    if not pre['safe']:print('runtime NOT_RUN: pressure/headroom guard');r['not_run']='pressure/headroom guard before guest or bottle copy';save();return 2
    lock=open('/tmp/betterxen-473-gpu.lock','a');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    bottle=args.bottle.expanduser().resolve();original=(bottle/'cxbottle.conf').read_bytes();temp=out/'bottle';shutil.copytree(bottle,temp,symlinks=True)
    conf=re.sub(r'(?m)^\[Bottle\]\s*$','[Bottle]\n"AllowUntrusted" = "1"',original.decode(),count=1);conf+='\n[EnvironmentVariables]\n"CX_GRAPHICS_BACKEND" = "d3dmetal"\n[CrossOver]\n"AntiVirusScan" = "untrusted"\n';(temp/'cxbottle.conf').write_text(conf)
    try:
        for i,seed in enumerate(seeds):
            for typ in (['wb','wc'] if i%2==0 else ['wc','wb']):
                pcheck=pressure()
                if not pcheck['safe']:r['stop_reason']=pcheck;save();raise RuntimeError('pressure guard before case')
                case=out/f'{typ}-seed-{seed}';case.mkdir();cmd=[str(cx/'bin/wine'),'--bottle',str(temp),'--no-update','--no-gui',str(exe),typ,str(seed)];c=dict(name=case.name,command=cmd,pressure_before=pcheck,snapshots=[]);r['cases'].append(c);save()
                with (case/'stdout.txt').open('wb') as stdout,(case/'stderr.txt').open('wb') as stderr:
                    p=subprocess.Popen(cmd,env=env,cwd=case,stdout=subprocess.PIPE,stderr=stderr,stdin=subprocess.PIPE,start_new_session=True);r['runtime_executed']=True
                    def kill():
                        try:os.killpg(p.pid,signal.SIGKILL)
                        except ProcessLookupError:pass
                    expired=threading.Event()
                    def expire():expired.set();kill()
                    timer=threading.Timer(120,expire);timer.start();pid=None
                    try:
                        for line in p.stdout:
                            stdout.write(line);stdout.flush()
                            if not line.startswith(b'STAGE '):continue
                            reading=pressure()
                            if not reading['safe']:
                                c['stop_pressure']=reading;save();raise RuntimeError('pressure guard at checkpoint')
                            if pid is None:
                                lookup=capture(['lsof','-t','-a','-c','reference.exe',exe]);pids=sorted({int(v) for v in lookup['stdout'].split() if v.isdigit()})
                                if len(pids)!=1:raise RuntimeError('ambiguous guest PID')
                                pid=pids[0];libs=capture(['lsof','-n','-p',str(pid)]);c['backend_paths_excerpt']=['/'+l.split('/',1)[1] for l in libs['stdout'].splitlines() if any(k in l for k in ['apple_gptk/wine/x86_64-windows/d3d12.dll','D3DMetal.framework/Versions/A/D3DMetal','/rosetta/runtime'])]
                            phase=line.decode().split()[1];sample=capture([observer,str(pid)]);snap=dict(phase=phase,pressure=reading,observer=sample)
                            for name,command in {'footprint':['footprint','-p',str(pid),'-f','bytes','--swapped'],'vmmap':['vmmap','-summary',str(pid)]}.items():
                                value=capture(command);(case/f'{phase}-{name}.json').write_text(json.dumps(value,indent=2)+'\n');snap[name]=dict(exit=value['exit'],file=f'{phase}-{name}.json')
                            c['snapshots'].append(snap);save();p.stdin.write(b'\n');p.stdin.flush()
                        p.wait()
                    except BaseException as e:c['harness_error']=repr(e);kill();p.wait();raise
                    finally:timer.cancel();c['exit']=p.returncode;c['timed_out']=expired.is_set();save()
                if c['exit']:raise RuntimeError('allocator case failed')
                print(case.name,'exit',c['exit'],flush=True)
    finally:
        stop=dict(env,WINEPREFIX=str(temp));r['server_stop']=helpers.capture([cx/'bin/wineserver','-k'],stop,out);r['server_wait']=helpers.capture([cx/'bin/wineserver','-w'],stop,out);r['source_bottle_unchanged']=(bottle/'cxbottle.conf').read_bytes()==original;shutil.rmtree(temp);save()
    return 0

if __name__=='__main__':raise SystemExit(main())
