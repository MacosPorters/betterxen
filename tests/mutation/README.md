# Native semantic mutation

**Preserved native research.** This fixture or validation procedure is for the paused independent native path, not CrossOver integration. Retain existing regression checks; proposed expansion is historical unless a specific hybrid need or maintainer decision authorizes it. See the [active architecture](../../docs/architecture.md).

Run serially on the native Apple Silicon gate machine.

```sh
CARGO_BUILD_JOBS=2 cargo run --locked -p xenora-semantic-review \
  --example semantic-mutation --features native-mutation
```

The runner checks the compiler's actual MSL against a reviewed straight-line
kernel before dispatching it. Each case compares the original reference and
Metal output with known answers derived from the synthetic ABI, integer
arithmetic and exact binary32 representation. The mutant is killed only if
Metal compiles it, runs it and reads back the correct number of words with at
least one changed value. Compilation, API, validation and deadline failures
end the campaign separately. The process exits 2 when a mutant survives.

| Cases | Change |
|---|---|
| Load and store | Substitute a loaded zero or the wrong store value, keeping both addresses lane-indexed. |
| Vector moves and unsigned add | Change a literal by one, zero-extend inline `-1`, emit subtraction or select the wrong operand. |
| Bitwise AND and XOR | Substitute a different bitwise operator. |
| Scalar comparison and forward branches | Substitute the lowerer's actual output for the wrong SCC path or retarget the unconditional branch to the following instruction. |
| FP32 add and multiply | Substitute multiplication for addition or addition for multiplication on reviewed finite values. |

The unsigned-add witness includes `u32::MAX`, so ordinary wraparound is
observable. Swapping operands in any supported binary operation is equivalent
in the verified domain. The register-only integer operators are commutative,
and the float subset has exact positive normal inputs and outputs with neither
NaNs nor signed zero. The semantic mutation uses the wrong source instead of
swapping operands. The compiler has no shift, carry-output or EXEC update
lowering in this subset. Load and store waits and termination are discharged by
verification. Their mutation coverage belongs in the source campaign. The
wrong-SCC case observes the combined scalar move, compare and conditional
branch. It does not distinguish which scalar lowering rule was faulty.

The runner accepts only embedded inputs and an enumerated set of edits.
Every dispatch uses six input words and six output words. Each thread can
access only its own lane and each output has one writer. Cases run serially
in separate child processes with a 30-second wall deadline and Metal API and
shader validation enabled. A timeout or fault stops the campaign. Keep raw
evidence under ignored `artifacts/` with the commit and toolchain environment.
