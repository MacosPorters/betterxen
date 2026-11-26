# Maintainer testing guide

How to build and independently test the supervised hybrid runtime and its Direct3D
workloads (issue #407). The workloads are original Windows programs. Passing them shows
narrow Windows API behaviour under CrossOver on one Mac. It says nothing about Xbox
software, game compatibility or performance, and no maintainer has reproduced these
results yet.

## What has been run

On commit `7451f0f2ae27b19313015d1ee529b69a755ef12f`, from an existing checkout rebuilt with
`build-maintainer.sh`, on one Apple M4 with CrossOver 26.3, by the author's coding agent.
This is not independent verification.

| Check | Result |
| --- | --- |
| fmt, clippy, tests | clean; 109 passed, 0 failed, 1 ignored (opt-in live D3DMetal test) |
| Shim build | 16 DLLs, 102 exports |
| `test_*.exe` fixtures, through `--bottle` and `--prefix` | passed, except `test_veh_unhandled.exe`, which crashes on purpose |
| Full acceptance suite | exit 2: 50 `ok`, 2 `KNOWN`, 0 `FAIL`, 0 `NOT-RUN` |
| On-screen capture checks | passed |

The two `KNOWN` lines are the D3D12 swapchain retention below. A clean-clone run and a
private-bundle run were last done on earlier source, before `main` was merged.

`main` at `02b8924` (`xmem` physical pages) was merged afterwards. The Rust checks, shim
build and all fixtures, now including `test_xmem_physical.exe`, were rerun with the same
results; the full acceptance suite was not.

After merging `main` at `bfa80de` (`bx-cli trace` reporting), the Rust checks (113 passed, 0
failed, 1 ignored), shim build and the shim export, CLWB and crash fixtures with their traces
were rerun.

## Requirements

- Apple Silicon Mac. Developed on an M4 with 24 GB and macOS 27; nothing else was tried.
- CrossOver 26.3 installed from CodeWeavers, and Rosetta 2. Neither is bundled.
- Xcode command line tools, `rustup`, Python 3.12 or newer.
- For the shim superbuild: `brew install mingw-w64 cmake`.
- At least 50% free memory (`memory_pressure -Q`) for the whole run. The runner refuses to
  start below that; close browsers and other large applications first.
- For the on-screen and keyboard checks: a desktop Space in front (not a full-screen app),
  an unlocked display, and Screen Recording and Accessibility permission for the terminal.
  Do not type while the keyboard check runs.

## Build

From a fresh clone of the branch under test:

```sh
TC=$(scripts/hybrid/install-dependencies.sh)          # pinned llvm-mingw and Rust 1.100.0
DXC=$(python3 scripts/hybrid/install-dxc.py)          # pinned Microsoft DXC, for Shader Model 6
export RUSTUP_TOOLCHAIN=1.100.0-aarch64-apple-darwin
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
scripts/build-shims.sh                                # 16 shim DLLs, 102 exports
scripts/hybrid/build-maintainer.sh "$TC"              # arm64 bx-cli and workloads
BX=$PWD/artifacts/maintainer/bin/bx-cli
```

Both installers download over the network, check a pinned SHA-256 and refuse to overwrite
an existing installation.

## Run

Use a new bottle that nothing else uses. Stopping a run stops everything in its bottle.

```sh
python3 scripts/hybrid/create-test-bottle.py "$BX" betterxen-maintainer-test
"$BX" doctor --bottle betterxen-maintainer-test
```

Shim fixtures (an explicit Wine prefix, separate from the bottle):

```sh
for t in test_veh_clwb test_shim_exports; do
  "$BX" run --prefix "$PWD/artifacts/m0-prefix" --shims build-shims/bin \
    --log "$PWD/artifacts/m0-shim.jsonl" --exe build-shims/bin/$t.exe
done
"$BX" trace artifacts/m0-shim.jsonl                   # expect "clwb VEH traps : at least 4 (sampled)", ADX 0
```

Full acceptance (Direct3D 12 and 11 workloads, negative controls, lifecycle, soak):

```sh
BETTERXEN_DXC_DIR="$DXC" scripts/hybrid/accept-integrated.sh "$TC" betterxen-maintainer-test
```

Each line is `ok`, `FAIL`, `KNOWN` or `NOT-RUN`. Records, logs and `summary.json` are
written under `artifacts/acceptance/<timestamp>/`.

| Exit status | Meaning |
| --- | --- |
| 0 | Every check ran and passed |
| 1 | At least one check failed |
| 2 | No failures, but the known D3D12 retention was seen or a check did not run |

On CrossOver 26.3 with D3DMetal, 2 is the best result available today.

## See the graphics

```sh
"$BX" run --bottle betterxen-maintainer-test -- \
  "$PWD/artifacts/maintainer/workloads/d3d12-integrated.exe" --frames 128 --hold-ms 15000
```

A small window titled "BetterXen D3D12 workload" appears on a black background:

- a 16-row purple band across the top (the frame number, encoded as a colour);
- an upper-left square split into green, blue (top) and red, white (bottom);
- a lower-right square split into cyan, magenta (top) and yellow, grey (bottom),
  overlapping the first and drawn in front of it.

The command then reports `failed_exit_1` with only the `shutdown` gate failed. That is the
known issue below, not a broken build. Any other failed gate is a new problem.

## Package (optional)

From a clean committed checkout, after `build-maintainer.sh`:

```sh
scripts/hybrid/package-maintainer.sh
```

It writes `artifacts/packages/betterxen-maintainer-<commit>.tar.gz` and its SHA-256. The
bundle contains `bx-cli`, the original workloads, source and licence notices. On another
Mac: check the SHA-256, unpack, and run `./test.sh NEW_BOTTLE_NAME` (set
`BETTERXEN_DXC_DIR` to include Shader Model 6). The binaries are not signed or notarized.

## Known limitations

- **API-set bridge shims (#437).** Under CrossOver, statically imported `api-ms-win-*` and
  `ext-ms-win-*` names are answered by Wine, not by the `win32_bridge` shims.
- **Bottle settings.** `[EnvironmentVariables]` in a bottle apply over variables given with
  `--env`; only `--backend` and DLL overrides are protected from this.
- **Intermittent D3D11 control stall.** One 20,000-frame soak stopped after 9,183 frames
  because a GPU event query did not complete within 10 s. Three reruns passed. The cause is
  unknown. The two D3D12 soaks of the same length did not show it.
- **Keyboard negative control.** It failed once inside a full run and passed twice on its
  own. The cause of that one failure was not captured.
- **D3D12 swapchain retention (D3DMetal).** A new swapchain starts with two references and
  survives the application's release, with no application GPU work. The same binary on the
  `vkd3d` path starts with one and is freed. Observed on one Mac; the cause inside D3DMetal
  is unknown and BetterXen has no mitigation. Programs that recreate swapchains will grow.
- **D3D12 on the `wined3d`/`dxvk` settings (`vkd3d`).** After about 437 frames every
  `Present` takes 2000 ms and teardown hangs. A 2000 ms wait in public Wine DXGI source is
  a likely explanation, not a confirmed one. Use the default D3DMetal path for D3D12.
- The earlier two-hour D3D12 run presented 844,497 frames with no mismatch in the verified
  frames, but failed the shutdown gate. It used a pre-migration build, so it does not
  certify this one.
- Double-precision shader operations are not exercised.
- Pixel checks read back what was handed to `Present`. Only the window-capture checks
  observe the macOS compositor.

## Security limits

A bottle or prefix is not a sandbox. A Windows program started this way can read and write
your files and use the network. `bx-cli` passes only a short list of environment variables
(`--inherit-env` opts out), supervises time, output and memory, and stops the selected
environment. Run only programs you trust.

## Troubleshooting

| Symptom | Action |
| --- | --- |
| `refused_to_start_low_memory` | Free memory and rerun; do not run other heavy work alongside |
| `bottle has an active BetterXen runner` | Stop that runner with Ctrl-C or SIGTERM |
| `bottle has an unfinished session` | `bx-cli stop --bottle NAME` (or `--prefix PATH`) |
| `the window exists but is not on the active Space` | Bring a desktop to the front and rerun |
| `NOT-RUN Shader Model 6` | Install DXC and set `BETTERXEN_DXC_DIR` |
| Crash control reports `timed_out` | Wine's crash dialog is waiting for a click. Create the bottle with `create-test-bottle.py`, which turns it off |

## Clean up

Stop any run with `bx-cli stop --bottle NAME`, then delete the test bottle from CrossOver.
Build output lives in `artifacts/`, `target/` and `build-shims/` inside the checkout;
installers use `~/Library/Caches/BetterXen/`.
