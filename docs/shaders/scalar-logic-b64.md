# SOP2 64-bit scalar logic

The gfx1030 decoder and synthetic compute compiler support eight two-source
scalar logical instructions. S0 and S1 are the first and second sources.
Every instruction sets the scalar condition code (SCC) if any of the 64
result bits is set, and clears SCC otherwise.

| Instruction | SOP2 opcode | Result |
| --- | --- | --- |
| `s_and_b64` | 15 | `S0 & S1` |
| `s_or_b64` | 17 | `S0 \| S1` |
| `s_xor_b64` | 19 | `S0 ^ S1` |
| `s_andn2_b64` | 21 | `S0 & ~S1` |
| `s_orn2_b64` | 23 | `S0 \| ~S1` |
| `s_nand_b64` | 25 | `~(S0 & S1)` |
| `s_nor_b64` | 27 | `~(S0 \| S1)` |
| `s_xnor_b64` | 29 | `~(S0 ^ S1)` |

The AMD RDNA 2 ISA reference guide gives the equations in §12.1 and the SOP2
fields in §13.1.1. Each scalar general-purpose register (SGPR) pair starts at
an even register from s0 to s104. The low word is in s[n] and the high word
in s[n+1] (§3.6.3 and §5.2). The operands are read before either result word
is written, so either source pair may alias the destination.

Inline selectors 128–208 supply integers from −16 to 64 (§5.2), with negative
values represented in 64-bit two's complement. One source may use selector
255 and consume a following literal dword. The pinned XML also defines a
`has_lit_0_has_lit_1` form, where both sources name the same literal; Xenora
does not accept it yet and rejects a second 255 selector (a subset restriction,
not a guide rule). The pinned AMD XML declares the
SOP2 logical literal operands as unsigned 64-bit values (`FMT_NUM_U64`). The
unsigned expansion rule in §6.2.1 gives them a zero high word even when bit
31 of the literal is set.

Odd pair bases, two literal selectors, floating-point inline constants and
special operands are unsupported. The special operands include the vector
condition code (VCC), execution mask (EXEC), M0, trap temporaries (TTMP),
NULL, SCC as a source, and other non-SGPR selectors. The synthetic compute
compiler also rejects writes to resource registers s0–s7 and reads from
either half of an uninitialized pair. Other 64-bit SOP2 operations, including
shifts and bit-field extraction, remain outside this subset.

The compiler resolves pairs and SCC at translation time and emits Metal
Shading Language for the resulting vector operations. The original
`scalar-logic-b64.s` fixture uses all eight instructions and folds their
results into an XOR key. Local CPU checks cover 4,432 oracle cases and 2,608
operand rejections (on both interpreters and the compiler), and the LLVM
three-way check covers fixture disassembly and instruction boundaries. Through
the translator, the 4,432 cases also pass on native Metal (Apple M5, API and
shader validation on: 185 programs). These are local checks against the guide,
XML and LLVM; independent acceptance is pending.

```sh
cargo test --locked -p xenora-semantic-review --test sop2_b64
MTL_DEBUG_LAYER=1 MTL_SHADER_VALIDATION=1 cargo test --locked -p xenora-semantic-review --test sop2_b64 sop2_b64_oracle_matches_native_metal -- --ignored
```
