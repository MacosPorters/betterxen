# Gate A: a compiled C program

Gate A asks for an independently compiled C program, exercising allocation,
file operations, arithmetic, function calls and error handling, to run through
Xenora. The maintainer split it into two milestones. Gate A is complete only
when both pass.

| Milestone | What it is | Status |
| --- | --- | --- |
| A1 | An original C program with no C library linked, importing documented `kernel32.dll` and `ucrtbase.dll` functions, run through the loader and the ARM64 DBT with proper process exit | Passes locally; awaiting independent review |
| A2 | A hosted C program: a real C runtime startup, the standard C library, `argc` and `argv`, stdio and formatted output, normal exit and runtime cleanup | Not started |

## A1

The program is [`tests/compat_fixtures/gate_a1.c`](../../tests/compat_fixtures/gate_a1.c).
Its build configuration and provenance are in
[`tests/compat_fixtures/README.md`](../../tests/compat_fixtures/README.md#gate-a1-a-c-program-with-no-c-library).
The gate is `crates/loader/tests/gate_a1.rs`:

```sh
cargo test --locked -p xenora-loader --test gate_a1 -- --nocapture
```

The test builds the program from source, then runs each build through
`run_synthetic_startup` with `register_standard_services`, loaded away from its
preferred base. It checks, at `-O0`, `-O1`, `-O2` and `-Os`:

- the 20 imports are bound and the 6 base relocations applied;
- the guest ends with `ExitReason::ProcessExit(0x00A1A1A1)`, all 32 bits;
- it ran as translated code, with service calls;
- the text report it wrote matches a report recomputed in Rust;
- the data file holds the 64 sorted records, the file it looked for does not exist, and the handle it left open was closed by the exit;
- its result block in guest memory holds the expected values, the block it kept is readable and the block it freed is unmapped.

Three controls must fail in the stated way: a wrong expectation exits with the
number of the failed check, an import that no service provides is refused
before entry, and a step budget that is too small is not an exit. A variant
whose entry point returns its status gives the same result as `ExitProcess`.

### What A1 does not show

- It is one program. Local runs used Homebrew clang 23.1.6 and Apple clang 17.0.0, each with LLD 23.1.5. Another compiler or a larger program can need instructions and services this one does not.
- Most services it calls are marked partial in the registry. They did what this program needs.
- It runs in the DBT only. The interpreter has no import dispatch.
- It reaches the guest through a test, not through `xenora run`, which accepts only two built-in fixtures.
- Nothing here concerns a commercial game or an Xbox launch environment.

## A2

Not started. It needs at least the C standard I/O functions, `exit` with
`atexit` handlers, command-line and environment access, and a decision on
which C runtime startup code may be linked and under what licence. None of
those are registered today.
