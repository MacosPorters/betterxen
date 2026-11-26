#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build and run B-local probes in a temporary CrossOver bottle."""
import argparse
import fcntl
import hashlib
import json
import os
import plistlib
import random
import re
import shutil
import signal
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
COHERENCE = HERE.parent / 'gpu-correctness' / 'coherence-and-tracking'
CROSSOVER = Path('/Applications/CrossOver.app/Contents/SharedSupport/CrossOver')
WINE = CROSSOVER / 'bin/wine'
FRAMEWORK = CROSSOVER / 'lib64/apple_gptk/external/D3DMetal.framework/Versions/A'
APPLE_GPTK_DLL = CROSSOVER / 'lib64/apple_gptk/wine/x86_64-windows/d3d12.dll'
COMPILER_DLL = CROSSOVER / 'lib/wine/x86_64-windows/d3dcompiler_47.dll'
BASELINE_SHA = '33e90bfee89e812f48990613cd74ced56473e5c3fc5d973f2ba1116090646a0c'
COMPILER = 'x86_64-w64-mingw32-g++'


def sha(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def capture(cmd, env, cwd, timeout=15):
    argv = [str(arg) for arg in cmd]
    try:
        result = subprocess.run(argv, env=env, cwd=cwd, text=True,
                                errors='replace', capture_output=True, timeout=timeout)
        return dict(command=argv, exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
    except (OSError, subprocess.TimeoutExpired) as failure:
        return dict(command=argv, exit=None, failure=repr(failure))


def pressure(env, cwd, max_level):
    kernel = capture(['sysctl', '-n', 'kern.memorystatus_vm_pressure_level'], env, cwd)
    free = capture(['memory_pressure', '-Q'], env, cwd)
    match = re.search(r'free percentage: (\d+)%', free.get('stdout', ''))
    level = kernel.get('stdout', '').strip()
    ok = (kernel['exit'] == free['exit'] == 0 and level.isdigit()
          and int(level) <= max_level and match and int(match[1]) >= 30)
    return dict(allowed=bool(ok), max_kernel_level=max_level, kernel=kernel, free=free,
                swap=capture(['sysctl', 'vm.swapusage'], env, cwd),
                vm=capture(['vm_stat'], env, cwd))


def cases_for(group, seed):
    if group == 'small':
        return [('copy', ['placement']), ('coherence', ['watch']), ('coherence', ['wrap-watch'])]
    if group == 'compute':
        names = ['wb', 'wc', 'upload-wb', 'default']
        random.Random(seed).shuffle(names)
        return [('coherence', [name]) for name in names]
    matrix = [('copy', ['sweep', heap, str(size), str(stride), str(seed)])
              for heap in ['wb', 'wc', 'upload'] for size in [1, 16, 64]
              for stride in [4096, 16384]]
    random.Random(seed).shuffle(matrix)
    return matrix


def ready(process, path, timeout):
    while time.monotonic() < timeout:
        if b'BACKEND_READY' in path.read_bytes():
            return
        if process.poll() is not None:
            raise RuntimeError('guest exited before backend marker')
        time.sleep(0.05)
    raise TimeoutError('backend marker')


def backend(exe, env, cwd):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        found = capture(['lsof', '-t', '-a', '-c', exe.name, str(exe)], env, cwd, 2)
        pids = {p for p in found.get('stdout', '').split() if p.isdigit()}
        if len(pids) == 1:
            pid = pids.pop()
            libs = capture(['lsof', '-n', '-p', pid], env, cwd, 3)
            markers = ('apple_gptk/wine/x86_64-windows/d3d12.dll',
                       'D3DMetal.framework/Versions/A/D3DMetal', '/rosetta/runtime',
                       'd3d12_xs.dll')
            paths = [line[line.index('/'):] for line in libs.get('stdout', '').splitlines()
                     if '/' in line and any(marker in line for marker in markers)]
            verified = (libs['exit'] == 0 and all(any(m in path for path in paths) for m in markers[:2])
                        and not any(markers[-1] in path for path in paths))
            return dict(verified=verified, guest_pid=pid, lsof_exit=libs['exit'], paths=paths)
        time.sleep(0.05)
    return dict(verified=False, message='could not identify unique probe PID', lookup=found)


def run_case(command, exe, label, has_gpu, case, env, max_level):
    case.mkdir()
    if label == 'coherence' and has_gpu:
        shutil.copy2(COHERENCE / 'access.hlsl', case)
    result = dict(command=command, timeout_seconds=45, exit=None)
    deadline = time.monotonic() + result['timeout_seconds']
    stdout = case / 'stdout.txt'
    process = None
    try:
        with stdout.open('wb') as out, (case / 'stderr.txt').open('wb') as err:
            process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=out, stderr=err,
                                       env=env, cwd=case, start_new_session=True)
            if has_gpu:
                ready(process, stdout, min(deadline, time.monotonic() + 20))
                result['backend'] = backend(exe, env, case)
                if not result['backend']['verified']:
                    raise RuntimeError('active D3DMetal verification failed before workload')
                result['pressure_ready'] = pressure(env, case, max_level)
                if not result['pressure_ready']['allowed']:
                    raise RuntimeError('pressure gate at paused device before workload')
                process.stdin.write(b'\n')
                process.stdin.flush()
            result['exit'] = process.wait(timeout=max(0, deadline - time.monotonic()))
    except BaseException as failure:
        result['error'] = repr(failure)
        if process is not None:
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            result['exit'] = process.wait(timeout=5)
        if isinstance(failure, KeyboardInterrupt):
            result['interrupted'] = True
    finally:
        result['output_hashes'] = {p.name: sha(p) for p in case.iterdir() if p.is_file()}
        (case / 'case.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bottle', type=Path)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--group', choices=['build', 'small', 'compute', 'sweep'], required=True)
    parser.add_argument('--seed', type=int, default=473)
    parser.add_argument('--allow-warning', action='store_true')
    args = parser.parse_args()
    if not 0 <= args.seed < 2**32:
        parser.error('seed must fit uint32')
    if args.group != 'build' and not args.bottle:
        parser.error('a win64 CrossOver bottle is required for execution')
    root = args.scratch.expanduser().resolve()
    if root == REPO or REPO in root.parents:
        parser.error('scratch must be outside the checkout')
    root.mkdir(mode=0o700)
    env = {k: os.environ[k] for k in ['HOME', 'USER', 'LOGNAME', 'PATH', 'LANG', 'LC_ALL'] if k in os.environ}
    env.update(TMPDIR=str(root) + '/', ROSETTA_ADVERTISE_AVX='1',
               CX_GRAPHICS_BACKEND='d3dmetal', WINEDEBUG='-all')
    receipt = dict(protocol='B-local-1', group=args.group, seed=args.seed,
                   utc=datetime.now(timezone.utc).isoformat(), independent_review='pending',
                   environment={k: env[k] for k in ['ROSETTA_ADVERTISE_AVX', 'CX_GRAPHICS_BACKEND', 'WINEDEBUG']},
                   commands={}, cases=[])
    source_files = [Path(__file__), HERE / 'support.h', HERE / 'probe.cpp',
                    COHERENCE / 'probe.cpp', COHERENCE / 'access.hlsl']
    receipt['sources'] = {}
    for path in source_files:
        relative = path.resolve().relative_to(REPO)
        snapshot = root / 'source' / relative
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, snapshot)
        receipt['sources'][str(relative)] = sha(snapshot)

    def save():
        (root / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')

    executables = {}
    tools = {'git': ['git', '-C', REPO, 'rev-parse', 'HEAD'], 'hardware': ['sysctl', '-n',
             'machdep.cpu.brand_string', 'hw.memsize', 'hw.pagesize'],
             'os': ['sw_vers'], 'host': ['uname', '-m'], 'cpu_load': ['uptime'],
             'rosetta': ['pkgutil', '--pkg-info', 'com.apple.pkg.RosettaUpdateAuto'],
              'compiler': [COMPILER, '--version'], 'compiler_arch': ['file', '-L', shutil.which(COMPILER) or COMPILER],
             'cx_version': [WINE, '--version'],
             'wine_engine': [CROSSOVER / 'bin/wineloader', '--version'],
             'python': ['python3', '--version']}
    for name, subcommand in tools.items():
        receipt['commands'][name] = capture(subcommand, env, root)
    receipt['baseline_sha256'] = sha(HERE.parent / 'probe.cpp')
    if receipt['baseline_sha256'] != BASELINE_SHA:
        save()
        raise RuntimeError('frozen baseline hash differs')
    receipt['framework'] = plistlib.loads((FRAMEWORK / 'Resources/Info.plist').read_bytes())
    receipt['runtime_sha256'] = {str(p): sha(p) for p in [APPLE_GPTK_DLL, COMPILER_DLL]}
    max_level = int(args.allow_warning)
    receipt['pressure_before'] = pressure(env, root, max_level)
    save()
    for name, path in [('copy', HERE), ('coherence', COHERENCE)]:
        exe = root / ('b-' + name + '.exe')
        source = root / 'source' / (path / 'probe.cpp').relative_to(REPO)
        cmd = [COMPILER, '-std=c++20', '-O2', '-static', '-static-libgcc', '-static-libstdc++',
               '-Wall', '-Wextra', '-Werror', source, '-o', exe]
        result = capture(cmd, env, root, 60)
        receipt['commands']['build_' + name] = result
        save()
        if result['exit'] != 0:
            return 1
        executables[name] = exe
        receipt.setdefault('executables', {})[name] = dict(filename=exe.name, sha256=sha(exe),
            format=capture(['file', exe], env, root))
    save()
    if args.group == 'build':
        return 0
    receipt['pressure_before_bottle'] = pressure(env, root, max_level)
    if not receipt['pressure_before_bottle']['allowed']:
        receipt['stop_reason'] = 'pressure gate before bottle clone; execution did not start'
        save()
        return 2
    lock = open('/tmp/betterxen-473-gpu.lock', 'a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    bottle = args.bottle.expanduser().resolve()
    config = bottle / 'cxbottle.conf'
    original = config.read_text()
    if '"WineArch" = "win64"' not in original:
        raise RuntimeError('requires a win64 bottle')
    receipt['bottle_input_sha256'] = sha(config)
    private = root / 'bottle'
    succeeded = True
    try:
        shutil.copytree(bottle, private, symlinks=True)
        clone_config = re.sub(r'(?m)^\[Bottle\]\s*$', '[Bottle]\n"AllowUntrusted" = "1"',
                              original, count=1)
        clone_config += '\n[EnvironmentVariables]\n"CX_GRAPHICS_BACKEND" = "d3dmetal"\n'
        clone_config += '\n[CrossOver]\n"AntiVirusScan" = "untrusted"\n'
        (private / 'cxbottle.conf').write_text(clone_config)
        receipt['bottle_settings'] = [line for line in clone_config.splitlines() if re.match(
            r'"(WineArch|Template|WindowsVersion|CX_GRAPHICS_BACKEND|AllowUntrusted|AntiVirusScan)"', line)]
        receipt['case_order'] = cases_for(args.group, args.seed)
        for index, (label, argv) in enumerate(receipt['case_order']):
            before = pressure(env, root, max_level)
            case = root / ('%02d-' % index + '-'.join(argv))
            item = dict(directory=case.name, pressure_before=before)
            receipt['cases'].append(item)
            if not before['allowed']:
                receipt['stop_reason'] = 'pressure gate before ' + case.name
                succeeded = False
                break
            exe = executables[label]
            command = [str(WINE), '--bottle', str(private), '--no-update', '--no-gui',
                       '--workdir', str(case), str(exe), *argv]
            result = run_case(command, exe, label, argv != ['watch'], case, env, max_level)
            after = pressure(env, root, max_level)
            item.update(run=result, pressure_after=after)
            save()
            succeeded &= result['exit'] == 0 and 'error' not in result
            if result.get('interrupted') or not after['allowed']:
                receipt['stop_reason'] = 'interruption or pressure gate after ' + case.name
                succeeded = False
                break
    finally:
        if private.is_dir():
            stop_env = env | {'WINEPREFIX': str(private), 'CX_BOTTLE_PATH': str(private)}
            receipt['commands']['private_wineserver_stop'] = capture(
                [CROSSOVER / 'bin/wineserver', '-k'], stop_env, root, 10)
            shutil.rmtree(private)
        receipt['bottle_unchanged'] = sha(config) == receipt['bottle_input_sha256']
        save()
    return 0 if succeeded else 1


if __name__ == '__main__':
    raise SystemExit(main())
