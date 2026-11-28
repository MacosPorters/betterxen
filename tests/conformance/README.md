# Synthetic PE32+ Conformance Tests (`tests/conformance/`)

Every directory matching `test_*/CMakeLists.txt` is **auto-discovered** by `tests/conformance/CMakeLists.txt` when running `scripts/build-shims.sh`.

## Adding a New Test (Zero Shared-File Edits)

1. Create `tests/conformance/test_<shim>/CMakeLists.txt` and `main.cpp`.
2. Build via `scripts/build-shims.sh` (outputs `build-shims/bin/test_<shim>.exe`).
3. Run live under CrossOver + Rosetta 2:
   ```sh
   BX_RUN="$(mktemp -d "${TMPDIR:-/tmp}/bx-run.XXXXXX")"   # a fresh prefix and log per run (AGENTS.md §8)
   target/debug/bx-cli run --prefix "$BX_RUN/prefix" --shims build-shims/bin --log "$BX_RUN/run.jsonl" --exe build-shims/bin/test_<shim>.exe
   ```
