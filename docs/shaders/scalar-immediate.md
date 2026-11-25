# SOPK scalar immediate forms

**Preserved native research.** This page governs existing synthetic fixtures and their regression checks. It does not define CrossOver compatibility or authorize native expansion; follow the [hybrid architecture](../architecture.md) for active work.

Xenora decodes and translates sixteen gfx1030 SOPK integer forms through the
semantic registry (`crates/isa/semantics/rdna2/sopk.toml`, operand rules
`sopk_sgpr` and `sopk_compare`). Each is one 32-bit word: SIMM16 in bits 0-15,
SDST in bits 16-22, OP in bits 23-27, ENCODING `0b1011` in bits 28-31
(guide 13.1.2). The word never takes a literal dword (guide 12.2), so a SIMM16
whose low byte is `0xff` is an immediate, not a literal marker.

| Form | Opcode | Operation | SCC |
| --- | ---: | --- | --- |
| `s_movk_i32` | 0 | D = signext(SIMM16) | unchanged |
| `s_cmovk_i32` | 2 | if SCC, D = signext(SIMM16); otherwise D keeps its value | read, unchanged |
| `s_cmpk_{eq,lg,gt,ge,lt,le}_i32` | 3-8 | SCC = (D relation signext(SIMM16)), signed | the relation |
| `s_cmpk_{eq,lg,gt,ge,lt,le}_u32` | 9-14 | SCC = (D relation SIMM16), SIMM16 zero-extended, unsigned | the relation |
| `s_addk_i32` | 15 | D = D + signext(SIMM16) | signed overflow of the sum |
| `s_mulk_i32` | 16 | D = low 32 bits of D * signext(SIMM16) | unchanged |

## Operands

SDST is `s0`-`s105`. `s0`-`s7` hold the resource descriptors of the fixture
ABI, so they may be read (the compares) but are never written: the translator
refuses a write there even when `s_cmovk_i32` would not take it. SDST 106-127
(`vcc`, `ttmp`, `m0`, `null`, `exec`) is rejected at decode. The other SOPK
opcodes stay unsupported, except the fixed store fence `s_waitcnt_vscnt null,
0` (`0xbbfd0000`), which keeps its own decoding.

## How it runs

SGPR values are known while translating, so a SOPK instruction folds into the
translator's scalar state exactly like the SOP2 and SOPC forms: no IR node is
emitted. `s_cmpk_*` sets SCC; `s_addk_i32` writes D and SCC; `s_mulk_i32` writes
D; `s_movk_i32` and a taken `s_cmovk_i32` write D. `s_addk_i32`, `s_mulk_i32`
and the compares read D, so an SDST nothing has written is rejected.
`s_cmovk_i32` reads SCC and is rejected when SCC is uninitialized; with SCC clear
it neither reads nor writes D, so an unwritten SDST stays unwritten.

## Verification

`tests/validation/tests/sopk.rs` derives its expected values from the guide
alone and runs 4,184 cases on both CPU interpreters and, through the translator,
on native Metal (that test is `#[ignore]`d and runs under `scripts/test-gpu.sh`),
plus 398 rejected programs. `fixtures/rdna2/scalar-immediate.s` (LLVM-assembled,
expected input ^ 0x40008275) uses all sixteen forms, recording each compare's SCC
through an `s_cbranch_scc0`; its IR and MSL are pinned under
`fixtures/golden/scalar-immediate.*`. Agreement with a spec-derived reference is
not agreement with hardware.
