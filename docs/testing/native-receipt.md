# Native ARM64 and Metal capability receipts

A native receipt is one contributor's reproducible host report, produced by
`scripts/audit/native-receipt.py`. It is evidence about the machine the
command ran on, not a gate, not a benchmark and not a claim that any other
Mac works. The tool is read-only: it installs nothing, posts nothing, and
never modifies another contributor's CI or build script.

## Running it

From the repository root on a native Apple Silicon Mac:

```sh
python3 scripts/audit/native-receipt.py \
  --cpu-command 'cargo test --locked -p xenora-loader --test cpu_known_answers' \
  --metal-command 'scripts/test-gpu.sh' \
  --output artifacts/native-receipt.json
```

Both fixture commands are optional. Use an existing original test target for
the CPU fixture (the example above runs the SDM-derived known-answer suite)
and an existing native Metal suite entry for the Metal fixture. Each command
runs from the repository root inside a bounded process (300 seconds by
default, `--timeout` to change). `--dependency PATH` adds an executable or
loadable file to inspect; without it the tool inspects the built `betterxen`
binary when one exists. `--no-matrix` omits the device matrix.

The report is written to `artifacts/native-receipt.json` (the directory is
ignored by git). The command exits 0 whenever the report was written,
whatever the fixture outcomes: the `overall` field inside the report is the
evidence, and a failing fixture is a recorded result, not a tool error.
A usage or I/O failure exits non-zero without claiming anything about the
host.

## What one receipt records

| Field | Content |
| --- | --- |
| `schema_version` | `1`. Bump and document when the shape changes. |
| `candidate_sha`, `dirty` | The inspected commit and whether the worktree had local changes. |
| `host` | `uname`, `sysctl.proc_translated` (Rosetta translation status), macOS version, CPU brand string, Python version. |
| `tools` | One-line versions of rustc, cargo, python3, clang and swift, or an explicit `unavailable`. |
| `metal` | The Metal shader toolchain location/version, the Xcode version, and the host's display report. This is runtime-observable host state only; the Apple feature-set table is a documented reference, not proof of execution on this host. |
| `validation` | Fixture timeout and the fact that Metal validation layers were not enabled by the receipt itself. |
| `commands` | The fixture commands exactly as run. |
| `dependencies` | `file`/`lipo -archs`/`otool -L` evidence for each inspected native dependency, or an explicit inspection failure. Non-arm64 architectures and Vulkan/MoltenVK linkage are rejected, never hidden. |
| `fixtures` | One entry per fixture: name, command, outcome, exit code and the last 500 bytes of each stream. |
| `matrix` | M1–M5 and an M6+ cell, each `unknown` until a contributor supplies a receipt for that generation; this host's cell reads `this host` when the CPU brand matches. |
| `overall` | `pass` when every fixture passed, `fail` when any fixture failed, `partial` when a fixture was skipped, unavailable or unsupported, and `unavailable` when no fixture ran. |

Fixture outcomes keep their own names: `pass`, `fail`, `skip`, `timeout`,
`unavailable` and `unsupported`. A skipped or unavailable Metal result is
recorded honestly and is never GPU verification. A zero-test or all-ignored
run is not a success.

## Evidence and limits

- A receipt proves execution on the host it names, at the recorded commit,
  with the recorded commands. It does not certify any other Apple Silicon
  generation, and one host report cannot support a cross-generation claim.
- Device capability evidence comes from running a Metal fixture, not from the
  Apple feature-set tables; the tables are a reference for what a documented
  generation supports.
- The report contains no user file contents, no game paths and performs no
  upload. Publishing a receipt is an explicit contributor action, outside
  this tool.
- Receipt collection is CPU-only and validated by
  `PYTHONPATH=scripts python3 -m unittest discover -s tests/qa` together with
  `tests/qa/test_native_receipt.py`, which covers a deliberately failing
  fixture, a missing command, a missing dependency path, a non-arm64
  artifact, JSON round-tripping and the unknown matrix cells. Actual Metal
  support still requires real device execution recorded through the Metal
  fixture.
- An implementer does not certify their own result: R1 audits claims and
  provenance, R2 independently re-derives expectations. Nothing here claims
  Xbox hardware verification or commercial-game compatibility.
