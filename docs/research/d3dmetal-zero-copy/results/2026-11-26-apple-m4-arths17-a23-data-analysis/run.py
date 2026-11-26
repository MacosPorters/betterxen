#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Workstream A local harness, version 2. Does not change shared protocols."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import signal
import subprocess
import threading
import time
from datetime import datetime, timezone


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def capture(cmd, env, cwd, timeout=20):
    start = time.monotonic()
    try:
        p = subprocess.run([str(v) for v in cmd], env=env, cwd=cwd,
                           capture_output=True, timeout=timeout)
        return dict(command=[str(v) for v in cmd], exit=p.returncode,
                    stdout=p.stdout.decode(errors='replace'),
                    stderr=p.stderr.decode(errors='replace'), seconds=time.monotonic()-start)
    except OSError as e:
        return dict(command=[str(v) for v in cmd], exit=127, stdout='', stderr=str(e))
    except subprocess.TimeoutExpired as e:
        return dict(command=[str(v) for v in cmd], exit=124, timeout=True,
                    stdout=(e.stdout or b'').decode(errors='replace'),
                    stderr=(e.stderr or b'').decode(errors='replace'))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--bottle', required=True, type=Path)
    ap.add_argument('--scratch', required=True, type=Path)
    ap.add_argument('--suite', choices=['memory', 'pool', 'texture', 'alias', 'render'], required=True)
    ap.add_argument('--smoke', action='store_true')
    ap.add_argument('--memory-sizes',default='64,256,1024')
    ap.add_argument('--memory-types',default='cpu-plain,cpu,wb,upload')
    ap.add_argument('--memory-repeats',type=int,choices=[1,2],default=2)
    ap.add_argument('--memory-repeat-start',type=int,choices=[0,1],default=0)
    ap.add_argument('--sessions',type=int,choices=[1,2,3],default=3,help='pool sessions')
    ap.add_argument('--session-start',type=int,choices=[0,1,2],default=0,help='pool trace session offset for nonduplicating continuation')
    ap.add_argument('--allow-warning', action='store_true', help='explicitly permit kernel warning level 1; urgent/critical still stop')
    ap.add_argument('--negative-control', action='store_true', help='pool omitted-copy or render omitted-draw control; expected exit 1')
    args = ap.parse_args()
    try:
        memory_sizes=[int(v) for v in args.memory_sizes.split(',')]
    except ValueError:
        ap.error('memory sizes must be comma-separated integer MiB values')
    memory_types=args.memory_types.split(',')
    if not memory_sizes or set(memory_sizes)-{64,256,1024} or not memory_types or set(memory_types)-{'cpu-plain','cpu','wb','upload'}:
        ap.error('memory profile must be an explicit subset of the common sizes/types')
    if len(set(memory_sizes)) != len(memory_sizes) or len(set(memory_types)) != len(memory_types):
        ap.error('memory profile must not contain duplicate sizes/types')
    if args.suite == 'memory' and args.memory_repeat_start + (1 if args.smoke else args.memory_repeats) > 2:
        ap.error('memory repeat indices must stay within the common two-session profile')
    if args.negative_control and args.suite not in ['pool','render']:
        ap.error('--negative-control only applies to pool/render')
    src = Path(__file__).resolve().parent
    repo = src.parents[3]
    base = args.scratch.expanduser().resolve()
    if base.exists() or base == repo or repo in base.parents:
        ap.error('scratch must be new and outside checkout')
    bottle = args.bottle.expanduser().resolve()
    original = (bottle/'cxbottle.conf').read_bytes()
    if b'"WineArch" = "win64"' not in original:
        ap.error('requires win64 bottle')
    lock = open('/tmp/betterxen-473-gpu.lock', 'a')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        ap.error('another Workstream A GPU batch owns the lock')
    base.mkdir(mode=0o700)
    env = {k: os.environ[k] for k in ['HOME', 'USER', 'LOGNAME', 'PATH', 'LANG', 'LC_ALL'] if k in os.environ}
    env.update(TMPDIR=str(base)+'/', ROSETTA_ADVERTISE_AVX='1', CX_GRAPHICS_BACKEND='d3dmetal', WINEDEBUG='-all')
    cx = Path('/Applications/CrossOver.app/Contents/SharedSupport/CrossOver')
    wine = cx/'bin/wine'
    source = src/(args.suite+'.cpp') if args.suite in ['memory','pool'] else src.parent/'gpu-correctness/textures-and-resources'/{'texture':'probe.cpp','alias':'alias.cpp','render':'render.cpp'}[args.suite]
    exe, observer = base/'a-probe.exe', base/'observe'
    receipt = dict(protocol='A-local-2.3', suite=args.suite, allow_warning=args.allow_warning, utc=datetime.now(timezone.utc).isoformat(),
                   ram_ceiling_bytes=23000000000, smoke=args.smoke, selected_memory_profile=(dict(sizes_MiB=[64] if args.smoke else memory_sizes,types=['wb'] if args.smoke else memory_types,repeats=1 if args.smoke else args.memory_repeats,repeat_start=args.memory_repeat_start) if args.suite=='memory' else None), independent_review='pending', environment={k: env[k] for k in ['ROSETTA_ADVERTISE_AVX', 'CX_GRAPHICS_BACKEND', 'WINEDEBUG']},
                   sources={str(p.relative_to(repo)): sha(p) for p in [source, src/'support.h', src/'observe.cpp', Path(__file__)]},
                   baseline_sha256=sha(src.parent/'probe.cpp'), tools={}, cases=[])
    snapshot=base/'source';snapshot.mkdir()
    for source_path in [source, src/'support.h', src/'observe.cpp', Path(__file__)]:
        destination = snapshot/source_path.relative_to(repo)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source_path, destination)
    def save():
        (base/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
    cmds = {
        'chip': ['sysctl','-n','machdep.cpu.brand_string'], 'ram':['sysctl','-n','hw.memsize'],
        'host':['uname','-m'], 'macos':['sw_vers'], 'page':['sysctl','-n','hw.pagesize'],
        'swap_before':['sysctl','vm.swapusage'], 'pressure_before':['memory_pressure','-Q'], 'vm_before':['vm_stat'],
        'load_before':['ps','-axo','pid,pcpu,comm'], 'wine_version':[wine,'--version'],
        'wine_build':[cx/'bin/wineloader','--version'],
        'kernel_pressure_before':['sysctl','-n','kern.memorystatus_vm_pressure_level'],
        'rosetta':['pkgutil','--pkg-info','com.apple.pkg.RosettaUpdateAuto'],
        'gcc':['x86_64-w64-mingw32-g++','--version'], 'clang':['xcrun','clang++','--version'],
        'sdk':['xcrun','--show-sdk-version'], 'python':['python3','--version'],
        'compiler_arch':['file',str(Path(shutil.which('x86_64-w64-mingw32-g++')).resolve())],
        'compiler_lipo':['lipo','-archs',str(Path(shutil.which('x86_64-w64-mingw32-g++')).resolve())],
        'compiler_links':['otool','-L',str(Path(shutil.which('x86_64-w64-mingw32-g++')).resolve())],
        'build_pe':['x86_64-w64-mingw32-g++','-std=c++20','-O2','-static','-static-libgcc','-static-libstdc++','-Wall','-Wextra','-Werror',source,'-o',exe,'-lkernel32'],
        'build_observer':['xcrun','clang++','-std=c++20','-O2','-arch','arm64','-Wall','-Wextra','-Werror',src/'observe.cpp','-o',observer],
        'formats':['file',exe,observer], 'observer_arch':['lipo','-archs',observer], 'observer_links':['otool','-L',observer],
    }
    for name, cmd in cmds.items():
        receipt['tools'][name]=capture(cmd,env,base,60)
        save()
        if name.startswith('build_') and receipt['tools'][name]['exit']:
            raise RuntimeError(receipt['tools'][name])
    receipt['binaries']={p.name:sha(p) for p in [exe,observer]}
    resources=cx/'lib64/apple_gptk/external/D3DMetal.framework/Versions/A/Resources'
    receipt['framework']={p.name:plistlib.loads(p.read_bytes()) for p in resources.glob('*.plist')}
    receipt['runtime_hashes']={str(p):sha(p) for p in [cx/'lib64/apple_gptk/wine/x86_64-windows/d3d12.dll', cx/'lib/wine/x86_64-windows/d3dcompiler_47.dll']}
    def headroom():
        vm=capture(['vm_stat'],env,base)
        ram=capture(['sysctl','-n','hw.memsize'],env,base)
        page=re.search(r'page size of (\d+) bytes',vm['stdout'])
        free=re.search(r'Pages free:\s+(\d+)',vm['stdout'])
        filebacked=re.search(r'File-backed pages:\s+(\d+)',vm['stdout'])
        if vm['exit'] or ram['exit'] or not all([page,free,filebacked]):
            raise RuntimeError('cannot measure host RAM headroom')
        # Report the metric precisely; this is not a claim to match Activity Monitor.
        used=max(0,int(ram['stdout'])-(int(free[1])+int(filebacked[1]))*int(page[1]))
        return dict(metric='physical RAM minus free and file-backed pages; conservative non-file-cache occupied estimate',
                    occupied_bytes=used,ceiling_bytes=23000000000,available_below_ceiling_bytes=23000000000-used,vm_stat=vm,ram=ram)

    preflight = headroom()
    kernel = capture(['sysctl','-n','kern.memorystatus_vm_pressure_level'],env,base)
    receipt['preflight_before_bottle_copy'] = dict(ram=preflight,kernel_pressure=kernel)
    if (preflight['available_below_ceiling_bytes'] < 512*1024**2 or kernel['exit'] or
        not kernel['stdout'].strip().isdigit() or
        int(kernel['stdout'].strip()) > (1 if args.allow_warning else 0)):
        receipt['stop_reason'] = 'preflight headroom/pressure guard; no bottle copied or guest started'
        receipt['source_bottle_unchanged'] = (bottle/'cxbottle.conf').read_bytes() == original
        save()
        return 2
    temp=base/'bottle'
    shutil.copytree(bottle,temp,symlinks=True)
    conf=re.sub(r'(?m)^\[Bottle\]\s*$', '[Bottle]\n"AllowUntrusted" = "1"', original.decode(), count=1)
    conf+='\n[EnvironmentVariables]\n"CX_GRAPHICS_BACKEND" = "d3dmetal"\n[CrossOver]\n"AntiVirusScan" = "untrusted"\n'
    (temp/'cxbottle.conf').write_text(conf)
    receipt['bottle_original_sha256']=hashlib.sha256(original).hexdigest()
    receipt['bottle_settings']=[line.strip() for line in conf.splitlines() if re.match(r'"(WineArch|Template|WindowsVersion|Version|CX_GRAPHICS_BACKEND|AllowUntrusted|AntiVirusScan)"',line)]
    common=[str(wine),'--bottle',str(temp),'--no-update','--no-gui',str(exe)]

    def run(name, argv, expected_exit=0):
        memory=headroom()
        reserve=512*1024**2+(2*int(argv[1]) if args.suite=='memory' else 0)
        if memory['available_below_ceiling_bytes']<reserve:
            receipt.setdefault('not_run',[]).append(dict(name=name,reason='23 GB headroom guard; no guest started',headroom=memory,required_reserve_bytes=reserve));save();return
        kernel_pressure=capture(['sysctl','-n','kern.memorystatus_vm_pressure_level'],env,base)
        if kernel_pressure['exit'] or not kernel_pressure['stdout'].strip().isdigit() or int(kernel_pressure['stdout'].strip())>(1 if args.allow_warning else 0):
            receipt['stop_reason']={'kernel_pressure_gate_before':name,'reading':kernel_pressure};save()
            raise RuntimeError('kernel memory pressure exceeds explicitly selected profile; no case started')
        pressure=capture(['memory_pressure','-Q'],env,base)
        match=re.search(r'free percentage: (\d+)%',pressure['stdout'])
        if pressure['exit'] or not match or int(match[1])<30:
            receipt['stop_reason']={'pressure_gate_before':name,'reading':pressure};save()
            raise RuntimeError('memory pressure headroom below 30% or unreadable')
        case=base/name;case.mkdir()
        metadata=dict(name=name,args=argv,expected_exit=expected_exit,command=common+argv,timeout_seconds=240,pressure_before=pressure,kernel_pressure_before=kernel_pressure,ram_before=memory,snapshots=[])
        receipt['cases'].append(metadata);save()
        expired=threading.Event()
        start=time.monotonic()
        with (case/'stdout.txt').open('wb') as out,(case/'stderr.txt').open('wb') as err:
            p=subprocess.Popen(common+argv,env=env,cwd=case,stdout=subprocess.PIPE,stderr=err,stdin=subprocess.PIPE,start_new_session=True)
            def kill():
                try: os.killpg(p.pid,signal.SIGKILL)
                except ProcessLookupError: pass
            def expire():
                expired.set();kill()
            timer=threading.Timer(240,expire);timer.start()
            pid=None
            try:
                for line in p.stdout:
                    out.write(line);out.flush()
                    if not line.startswith(b'STAGE '):continue
                    if pid is None:
                        found=capture(['lsof','-t','-a','-c','a-probe.exe',str(exe)],env,base)
                        candidates=sorted({int(x) for x in found['stdout'].split() if x.isdigit()})
                        if len(candidates)!=1:raise RuntimeError('ambiguous PE PID: '+repr(found))
                        pid=candidates[0];metadata['host_pid']=pid
                        libs=capture(['lsof','-n','-p',str(pid)],env,base)
                        metadata['backend_paths_excerpt']=['/'+line.split('/',1)[-1] for line in libs['stdout'].splitlines() if any(x in line for x in ['apple_gptk/wine/x86_64-windows/d3d12.dll','D3DMetal.framework/Versions/A/D3DMetal','/rosetta/runtime'])]
                        metadata['backend_lsof_exit']=libs['exit']
                    phase=line.decode().split()[1]
                    snap=dict(phase=phase,marker=line.decode().strip(),samples=[],ram=headroom())
                    if snap['ram']['available_below_ceiling_bytes']<256*1024**2:
                        raise RuntimeError('stop before 23 GB host occupied-memory ceiling')
                    for _ in range(3):
                        result=capture([observer,str(pid)],env,base)
                        if not result['exit']:result['data']=json.loads(result['stdout'])
                        snap['samples'].append(result)
                    commands={'footprint':['footprint','-p',str(pid),'-f','bytes','--swapped'],
                              'vmmap':['vmmap','-summary',str(pid)], 'vmstat':['vm_stat'],
                              'pressure':['memory_pressure','-Q'],'swap':['sysctl','vm.swapusage'],
                              'kernel_pressure':['sysctl','-n','kern.memorystatus_vm_pressure_level']}
                    ptr=re.search(rb'cpu_ptr=([0-9A-Fa-f]+)',line)
                    if ptr and int(ptr[1],16):commands['pointer_vmmap']=['vmmap','-w',str(pid),'0x'+ptr[1].decode()]
                    for tool,cmd in commands.items():
                        result=capture(cmd,env,base,15)
                        filename=f'{phase}-{tool}.json';(case/filename).write_text(json.dumps(result,indent=2)+'\n')
                        snap[tool]=dict(exit=result['exit'],file=filename)
                        if tool=='kernel_pressure' and (result['exit'] or not result['stdout'].strip().isdigit() or int(result['stdout'].strip())>(1 if args.allow_warning else 0)):
                            raise RuntimeError('kernel pressure changed; stop this case without resuming guest')
                        if tool=='pressure':
                            m=re.search(r'free percentage: (\d+)%',result['stdout'])
                            if result['exit'] or not m or int(m[1])<25:raise RuntimeError('stage pressure safety stop')
                    metadata['snapshots'].append(snap);save()
                    p.stdin.write(b'\n');p.stdin.flush()
                p.wait()
            except BaseException as e:
                metadata['harness_error']=repr(e);kill();p.wait();raise
            finally:
                timer.cancel();metadata.update(exit=p.returncode,timed_out=expired.is_set(),seconds=time.monotonic()-start)
                metadata['outputs']={f.name:sha(f) for f in case.iterdir() if f.is_file()}
                (case/'case.json').write_text(json.dumps(metadata,indent=2)+'\n');save()
        print(name,'exit',metadata['exit'],'timeout',metadata['timed_out'],flush=True)
        if metadata['timed_out']:raise RuntimeError('case timed out: '+name)
        if metadata['exit']!=expected_exit:
            receipt.setdefault('failed_cases',[]).append(name);save()
            if args.suite not in ['texture','alias','render']:raise RuntimeError('case failed: '+name)
        if expected_exit and (b'END failure reason=sampled pixels mismatch' if args.suite=='render' else b'END failure reason=trace guard/interior mismatch') not in (case/'stdout.txt').read_bytes():
            raise RuntimeError('negative control failed for an unexpected reason')
    try:
        if args.suite=='memory':
            for repeat in range(args.memory_repeat_start,args.memory_repeat_start+(1 if args.smoke else args.memory_repeats)):
                sizes=[64] if args.smoke else list(memory_sizes)
                types=['wb'] if args.smoke else list(memory_types)
                if repeat%2:types.reverse();sizes.reverse()
                for size in sizes:
                    for typ in types:run(f'memory-{typ}-{size}MiB-r{repeat}',[typ,str(size*1024**2)])
        elif args.suite=='render' and args.negative_control:
            run('render-omitted-draw-control',['omit'],1)
        elif args.suite=='pool' and args.negative_control:
            run('pool-omitted-copy-control',['omit','473'],1)
        elif args.suite=='pool':
            for session in range(args.session_start,args.session_start+(1 if args.smoke else args.sessions)):
                modes=['individual','pooled'] if session%2==0 else ['pooled','individual']
                for mode in modes:run(f'pool-{mode}-s{session}',[mode,str(473+session)])
        else:
            for session in range(1 if args.smoke else 2):run(f'{args.suite}-s{session}',[str(session)])
    finally:
        for name,cmd in {'pressure_after':['memory_pressure','-Q'],'vm_after':['vm_stat'],'swap_after':['sysctl','vm.swapusage'],'kernel_pressure_after':['sysctl','-n','kern.memorystatus_vm_pressure_level']}.items():receipt['tools'][name]=capture(cmd,env,base)
        stop=dict(env,WINEPREFIX=str(temp))
        receipt['server_stop']=capture([cx/'bin/wineserver','-k'],stop,base)
        receipt['server_wait']=capture([cx/'bin/wineserver','-w'],stop,base)
        receipt['source_bottle_unchanged']=(bottle/'cxbottle.conf').read_bytes()==original
        shutil.rmtree(temp);save()
    return int(bool(receipt.get('failed_cases') or receipt.get('not_run')))

if __name__=='__main__':
    raise SystemExit(main())
