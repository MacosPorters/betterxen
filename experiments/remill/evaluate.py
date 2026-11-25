#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build/run the trusted, isolated Remill experiment; write receipts under artifacts/."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import statistics
import subprocess
import time

PIN = "66c488c9911fd9b368e4d0c9647d74239f8bba8b"
HERE = Path(__file__).resolve().parent
FIXTURES = {
    "add": "4801c8488903c3",  # add rax,rcx; mov [rbx],rax; ret
    "load": "488b03c3",       # mov rax,[rbx]; ret
    "store": "488903c3",      # mov [rbx],rax; ret
    "ud2": "0f0b",           # deliberately invalid guest instruction
    "jump": "ffe0",          # indirect jmp rax; unsupported by our dispatcher
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True,
                        help="artifact directory containing upstream/ at the pinned revision")
    parser.add_argument("--llvm", type=Path, default=Path("/opt/homebrew/opt/llvm@17"))
    parser.add_argument("--extended", action="store_true", help="run SIMD/control probes and matched integer measurements")
    parser.add_argument("--build", action="store_true", help="configure/build dependencies and Remill first")
    args = parser.parse_args()
    root, llvm = args.root.resolve(), args.llvm.absolute()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("this experiment requires native arm64 macOS")
    # Never place generated source or receipts in a tracked directory.
    root.mkdir(parents=True, exist_ok=True)
    check = subprocess.run(["git", "check-ignore", str(root / "receipt.json")],
                           cwd=root, capture_output=True)
    if check.returncode:
        parser.error("--root must be inside an ignored artifacts/ directory")
    host_version = platform.mac_ver()[0]
    receipt = {"remill_revision": PIN, "platform": platform.platform(),
               "commands": [], "native": [], "outcome": "incomplete"}
    env = dict(os.environ, CMAKE_BUILD_PARALLEL_LEVEL="2", CARGO_BUILD_JOBS="2",
               CARGO_TARGET_DIR=str(root / "bridge-target"))

    def run(name, argv, timeout=180, cwd=root, expected_exit=0):
        if args.extended:
            name = "extended-" + name
        argv = list(map(str, argv))
        start = time.monotonic()
        record = {"name": name, "argv": argv, "cwd": str(cwd)}
        receipt["commands"].append(record)
        try:
            proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, start_new_session=True)
            try:
                output, _ = proc.communicate(timeout=timeout)
            except subprocess.TimeoutExpired:
                # Bound the entire build process group, including nested Ninja jobs.
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    output, _ = proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    output, _ = proc.communicate()
                (root / (name + ".log")).write_bytes(output)
                record.update(exit="timeout", seconds=time.monotonic() - start)
                raise RuntimeError(f"{name} exceeded {timeout} seconds")
            (root / (name + ".log")).write_bytes(output)
            record.update(exit=proc.returncode, seconds=time.monotonic() - start)
            record["expected_exit"] = expected_exit
            if proc.returncode != expected_exit:
                raise RuntimeError(f"{name} failed ({proc.returncode}); see {root / (name + '.log')}")
            return output.decode(errors="replace")
        except OSError:
            record.update(exit="could not execute", seconds=time.monotonic() - start)
            raise

    audited = set()

    def audit(path):
        path = Path(path).resolve()
        if path in audited:
            return
        audited.add(path)
        name = "native-" + hashlib.sha256(str(path).encode()).hexdigest()[:12]
        description = run(name + "-file", ["/usr/bin/file", path])
        archs = run(name + "-archs", ["/usr/bin/lipo", "-archs", path]).strip()
        if not any(a.startswith("arm64") for a in archs.split()):
            raise RuntimeError(f"refusing non-ARM64 native input: {path}")
        item = {"path": str(path), "archs": archs, "file": description.strip()}
        receipt["native"].append(item)
        # Archives have no dynamic load commands. Check their ARM64 slice above.
        if path.suffix == ".a":
            return
        links = run(name + "-links", ["/usr/bin/otool", "-L", path])
        commands = run(name + "-load", ["/usr/bin/otool", "-l", path])
        rpaths = re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.*?) \(offset", commands)

        def expand(value):
            return value.replace("@loader_path", str(path.parent)).replace("@executable_path", str(path.parent))

        dependencies = []
        for line in links.splitlines()[1:]:
            if " (compatibility version" not in line:
                continue
            lib = line.strip().split(" (compatibility")[0]
            if lib.startswith(("/usr/lib/", "/System/Library/")):
                dependencies.append({"install_name": lib, "system_shared_cache": True})
                continue
            candidates = [Path(expand(lib))]
            if lib.startswith("@rpath/"):
                candidates = [Path(expand(r)) / lib[7:] for r in rpaths]
                candidates += [llvm / "lib" / lib[7:]]
            resolved = next((p for p in candidates if p.exists()), None)
            if resolved is None:
                raise RuntimeError(f"unresolved native dependency: {path}: {lib}")
            dependencies.append({"install_name": lib, "resolved": str(resolved.resolve())})
            audit(resolved)
        item["dependencies"] = dependencies

    try:
        upstream = root / "upstream"
        revision = run("upstream-revision", ["git", "rev-parse", "HEAD"], cwd=upstream).strip()
        if revision != PIN or run("upstream-status", ["git", "status", "--porcelain"], cwd=upstream).strip():
            raise RuntimeError("upstream must be clean and match PIN")
        receipt["project_revision"] = run("project-revision", ["git", "rev-parse", "HEAD"], cwd=HERE).strip()
        receipt["fixture_bytes"] = FIXTURES
        receipt["source_sha256"] = {str(p.relative_to(HERE)): hashlib.sha256(p.read_bytes()).hexdigest()
                                    for p in [Path(__file__), HERE / "prototype.cpp", HERE / "bridge/src/lib.rs", HERE / "bridge/Cargo.lock"]}
        for tool in ["cmake", "ninja", "git", "make", "rustup"]:
            audit(shutil.which(tool))
        for tool in ["clang", "clang++", "llvm-ar", "llvm-link", "llc", "opt", "llvm-mc", "llvm-nm", "llvm-objcopy", "llvm-objdump"]:
            audit(llvm / "bin" / tool)
        # Check real toolchain executables, not only the rustup proxy.
        for tool in ["cargo", "rustc"]:
            path = run("which-" + tool, ["rustup", "which", "--toolchain", "1.100.0", tool]).strip()
            audit(path)
        audit(shutil.which("python3"))

        if args.build:
            common = [f"-DCMAKE_C_COMPILER={llvm}/bin/clang",
                      f"-DCMAKE_CXX_COMPILER={llvm}/bin/clang++", "-DCMAKE_OSX_ARCHITECTURES=arm64"]
            run("deps-configure", ["cmake", "-G", "Ninja", "-S", upstream / "dependencies", "-B", root / "deps-build",
                                  "-DUSE_EXTERNAL_LLVM=ON", "-DENABLE_SLEIGH=ON", f"-DCMAKE_PREFIX_PATH={llvm}",
                                  f"-DCMAKE_INSTALL_PREFIX={root}/deps-install", *common])
            run("deps-build", ["cmake", "--build", root / "deps-build", "--parallel", "2"], timeout=1800)
            run("remill-configure", ["cmake", "-G", "Ninja", "-S", upstream, "-B", root / "remill-build",
                                    f"-DCMAKE_PREFIX_PATH={root}/deps-install;{llvm}",
                                    f"-DCMAKE_INSTALL_PREFIX={root}/remill-install", "-DCMAKE_BUILD_TYPE=Release",
                                    "-DREMILL_FETCH_SLEIGH=OFF", "-DREMILL_ENABLE_TESTING=OFF", *common])
            run("remill-build", ["cmake", "--build", root / "remill-build", "--parallel", "2"], timeout=1800)
        dependency_sources = {
            "gflags": ("gflags-prefix/src/gflags", "52e94563eba1968783864942fedf6e87e3c611f4"),
            "glog": ("glog-prefix/src/glog", "7b134a5c82c0c0b5698bb6bf7a835b230c5638e4"),
            "googletest": ("googletest-prefix/src/googletest", "52eb8108c5bdec04579160ae17225d66034bd723"),
            "xed": ("xed-prefix/src/xed", "dc6bdbe036515c7ef55d3b2d50bafc5232bfe534"),
            "mbuild": ("xed-prefix/src/mbuild", "7c4497f41f576b43a80bb0f8d8452bbcfd58b6e2"),
            "sleigh": ("sleigh-prefix/src/sleigh", "7c6b7424467d0382a1303c278633e99b0d094d5b"),
        }
        receipt["dependency_revisions"] = {}
        for name, (relative, expected) in dependency_sources.items():
            path = root / "deps-build" / relative
            actual = run("revision-" + name, ["git", "rev-parse", "HEAD"], cwd=path).strip()
            if actual != expected:
                raise RuntimeError(f"dependency revision mismatch: {name}: {actual}")
            receipt["dependency_revisions"][name] = actual
        ghidra = root / "deps-build/sleigh-prefix/src/sleigh-build/_deps/ghidrasource-src"
        base = "80ccdadeba79cd42fb0b85796b55952e0f79f323"
        run("ghidra-base", ["git", "merge-base", "--is-ancestor", base, "HEAD"], cwd=ghidra)
        receipt["dependency_revisions"]["ghidra_base"] = base
        receipt["ghidra_patch_commits"] = run("ghidra-patches", ["git", "log", "--format=%H %s", base + "..HEAD"], cwd=ghidra)
        receipt["upstream_patch_sha256"] = {str(p.relative_to(upstream)): hashlib.sha256(p.read_bytes()).hexdigest()
                                              for p in (upstream / "dependencies/sleigh_patches").glob("*.patch")}
        cache = (root / "deps-build/CMakeCache.txt").read_text()
        python_path = re.search(r"^_Python3_EXECUTABLE:INTERNAL=(.+)$", cache, re.MULTILINE)
        if python_path:
            audit(python_path.group(1))
        run("remill-install", ["cmake", "--install", root / "remill-build"])
        lifter = root / "remill-install/bin/remill-lift-17"
        audit(lifter)
        for binary in (root / "deps-install/bin").iterdir():
            if binary.is_file() and binary.open("rb").read(4) in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe"):
                audit(binary)
        for archive in (root / "deps-install/lib").glob("*.a"):
            audit(archive)
        release = ["--release"] if args.extended else []
        run("bridge-build", ["cargo", "+1.100.0", "build", *release, "--locked", "--manifest-path", HERE / "bridge/Cargo.toml",
                             "--target", "aarch64-apple-darwin"], timeout=300, cwd=HERE)
        profile = "release" if args.extended else "debug"
        bridge = root / f"bridge-target/aarch64-apple-darwin/{profile}/libbetterxen_remill_experiment.a"
        audit(bridge)
        fixtures = dict(FIXTURES)
        if args.extended:
            fixtures.update(addss="f30f58c1c3", paddd="660ffec1c3", vpaddd="c5fdfec1c3", vpxor="c5f9efc0c3",
                            loop="48ffc048ffc975f8488903c3", register256="4801c8" * 256 + "488903c3",
                            memory256="4801c8488903" * 256 + "c3")
            run("fixture-assemble", [llvm / "bin/llvm-mc", "-triple=x86_64-unknown-linux-gnu", "-filetype=obj",
                                     HERE / "fixtures.s", "-o", root / "extended-fixtures.o"])
            run("fixture-extract", [llvm / "bin/llvm-objcopy", "--only-section=.text", "-O", "binary",
                                    root / "extended-fixtures.o", root / "extended-fixtures.bin"])
            symbols = run("fixture-symbols", [llvm / "bin/llvm-nm", "--numeric-sort", "--defined-only", root / "extended-fixtures.o"])
            offsets = [(int(line.split()[0], 16), line.split()[-1]) for line in symbols.splitlines()]
            raw = (root / "extended-fixtures.bin").read_bytes()
            aliases = {"bench_register": "register256", "bench_memory": "memory256"}
            for index, (offset, symbol) in enumerate(offsets):
                name = aliases.get(symbol, symbol.removesuffix("_probe"))
                end = offsets[index+1][0] if index+1 < len(offsets) else len(raw)
                if raw[offset:end].hex() != fixtures[name]:
                    raise RuntimeError(f"original fixture assembler mismatch: {name}")
            receipt["source_sha256"].update({name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                                             for name in ["probes.cpp", "fixtures.s", "bridge/Cargo.toml"]})
        receipt["fixture_bytes"] = fixtures
        objects = []
        for name, code in fixtures.items():
            ir = root / (name + ".ll")
            run("lift-" + name, [lifter, "--arch", "amd64_avx" if name in {"addss", "paddd", "vpaddd", "vpxor"} else "amd64", "--os", "linux", "--address", "4096",
                                "--bytes", code, "--ir_out", ir])
            source = ir.read_text()
            if "@sub_1000(" not in source:
                raise RuntimeError("entry symbol missing from lifted output")
            # Preserve the lifted body. Override only the entry name and host codegen attributes.
            # Guest x86 target-features must not be handed to the ARM64 backend.
            source = source.replace("@sub_1000(", "@lifted_" + name + "(")
            source = re.sub(r' "target-(?:cpu|features|tune-cpu)"="[^"]*"', "", source)
            native_ir = root / (name + "-arm64.ll")
            native_ir.write_text(source)
            obj = root / (name + ".o")
            run("lower-" + name, [llvm / "bin/llc", f"-mtriple=arm64-apple-macosx{host_version}", "-filetype=obj", "-O2", native_ir, "-o", obj])
            audit(obj)
            if args.extended and name in {"register256", "memory256"}:
                sections = run("sections-" + name, [llvm / "bin/llvm-objdump", "--section-headers", obj])
                match = re.search(r"^\s*\d+ __text\s+([0-9a-fA-F]+)\s", sections, re.MULTILINE)
                if not match:
                    raise RuntimeError("native text section missing")
                receipt.setdefault("remill_text_bytes", {})[name] = int(match.group(1), 16)
            objects.append(obj)
        executable = root / ("prototype-extended" if args.extended else "prototype")
        run("prototype-link", [llvm / "bin/clang++", "-std=c++17", "-O2", f"-mmacosx-version-min={host_version}", "-Wall", "-Wextra", "-Werror", "-arch", "arm64", "-DADDRESS_SIZE_BITS=64",
                               f"-DHAS_FEATURE_AVX={int(args.extended)}", "-DHAS_FEATURE_AVX512=0", "-I", upstream / "include",
                               HERE / ("probes.cpp" if args.extended else "prototype.cpp"), *objects, bridge, "-framework", "Security", "-framework", "CoreFoundation",
                               "-liconv", "-lresolv", "-o", executable])
        audit(executable)
        semantic_error = None
        try:
            output = run("prototype-run", [executable], timeout=30)
        except RuntimeError as error:
            if not args.extended:
                raise
            semantic_error = str(error)
            output = (root / "extended-prototype-run.log").read_text()
        if args.extended:
            # Integer measurements have their own per-run correctness gate. A failed SIMD
            # contract remains a failed gate even if these separate measurements succeed.
            receipt["measurement_limits"] = "Exploratory active-desktop run; not an idle-machine benchmark. Includes reset/check/return glue."
            measurements = run("benchmark", [executable, "--benchmark"], timeout=120)
            receipt["measurements"] = [json.loads(line) for line in measurements.splitlines()]
            receipt["measurement_summary"] = {}
            for workload in ["register256", "memory256"]:
                receipt["measurement_summary"][workload] = {}
                for engine in range(4):
                    rows = [r for r in receipt["measurements"] if r["kind"] == "sample" and r["workload"] == workload and r["engine"] == engine]
                    if len(rows) != 9:
                        raise RuntimeError("incomplete timing sample set")
                    values = [r["ns"] / r["repetitions"] / 1000 for r in rows]
                    receipt["measurement_summary"][workload][engine] = {"median_us": statistics.median(values), "min_us": min(values), "max_us": max(values)}
            try:
                run("runaway-budget", [executable, "--runaway"], timeout=1)
                receipt["runaway_budget"] = "returned; inspect exit/log"
            except RuntimeError as error:
                receipt["runaway_budget"] = str(error)
                semantic_error = (semantic_error + "; " if semantic_error else "") + str(error)
            receipt["semantic_observations"] = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
        run("wrong-answer-control", [executable, "--wrong-answer"], timeout=30, expected_exit=1)
        receipt["outcome"] = "local prototype passed; independent acceptance pending"
        receipt["output"] = output.strip()
        receipt["executable_sha256"] = hashlib.sha256(executable.read_bytes()).hexdigest()
        print(output, end="")
        if semantic_error:
            raise RuntimeError(semantic_error)
    except Exception as error:
        receipt["outcome"] = "failed"
        receipt["error"] = str(error)
        raise
    finally:
        (root / ("receipt-extended.json" if args.extended else "receipt.json")).write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
