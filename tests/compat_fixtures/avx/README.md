# Original AVX integer fixture specification

**Preserved native research.** This fixture or validation procedure is for the paused independent native path, not CrossOver integration. Retain existing regression checks; proposed expansion is historical unless a specific hybrid need or maintainer decision authorizes it. See the [active architecture](../../../docs/architecture.md).

This fixture is a proposed known-answer test for the
[guest AVX contract](../../../docs/cpu/avx-contract.md). It uses only the
source below, fixed register values and synthetic guest memory. Run the
eventual fixture in both guest engines on fresh copies of the same initial
state. Use the independent expected values below for each engine.

## Guest sequence

Set `r8` and `r9` to separate readable ranges containing arrays A and B.
Set `r10` to a writable 96-byte output region initialized to `0xA5` with
mapped sentinel bytes before and after it. Map the code separately as guest
executable memory. Initialize flags, MXCSR and untouched vector registers to
known nonzero values, then record their initial values. The sequence uses no
stack, imports or host calls.

```asm
.intel_syntax noprefix
.text
vmovdqu ymm1, ymmword ptr [r8]
vmovdqu ymm2, ymmword ptr [r9]
vpaddd ymm3, ymm1, ymm2
vmovdqu ymmword ptr [r10], ymm3
movdqu xmm3, xmm1
vmovdqu ymmword ptr [r10 + 32], ymm3
vmovdqu xmm3, xmm2
vmovdqu ymmword ptr [r10 + 64], ymm3
hlt
```

The high base registers require three-byte VEX for the memory moves, while
the low-register `VPADDD` and register `VMOVDQU` can use two-byte VEX.
Store each table row as eight little-endian 32-bit words, with lane 0 at
the lowest address. All entries are hexadecimal.

| Words | Lane 0 | Lane 1 | Lane 2 | Lane 3 | Lane 4 | Lane 5 | Lane 6 | Lane 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A | `ffffffff` | `7fffffff` | `80000000` | `00000000` | `00000010` | `fffffffe` | `12345678` | `aaaaaaaa` |
| B | `00000001` | `00000001` | `80000000` | `ffffffff` | `00000020` | `00000003` | `11111111` | `55555556` |
| Output +0 | `00000000` | `80000000` | `00000000` | `ffffffff` | `00000030` | `00000001` | `23456789` | `00000000` |
| Output +32 | `ffffffff` | `7fffffff` | `80000000` | `00000000` | `00000030` | `00000001` | `23456789` | `00000000` |
| Output +64 | `00000001` | `00000001` | `80000000` | `ffffffff` | `00000000` | `00000000` | `00000000` | `00000000` |

For output +0, add corresponding words as unsigned integers and discard
bits above bit 31 in each sum. For example, lane 0 wraps `0x100000000` to
zero, so lane 1 has no incoming carry. At +32, the legacy move replaces the
low four words with A and retains the high four words of the sum. At +64,
the VEX.128 move replaces the low four words with B and clears the high
four. Final YMM1 and YMM2 equal A and B. Final YMM3 equals output +64.
All other vector registers, the input bytes, sentinels, general-purpose
registers, flags and MXCSR retain their initial values. Expect `Halt` with
nine steps, including the terminal `HLT`.

## Controls

| Case | Expected result |
| --- | --- |
| `VPADDD` writes to one of its input registers | Same eight-word sum using both inputs as they stood before the instruction. The other source survives. |
| 128-bit `VPADDD` with seeded destination upper half | Output +0's low four words and four zero words. Both sources survive when distinct from the destination. |
| 128-bit `VMOVDQU` store with sentinels | Exactly the low 16 bytes are stored. The entire source YMM and memory outside the operand survive. |
| 256-bit `VMOVDQU` at a 32-byte-aligned address, at an address offset by one byte, and at `P-16` across adjacent accessible mappings | Same 32 bytes at each address. `P` is the start of the second mapping. |
| Split load at `P-16`, with the second mapping unreadable or unmapped | Memory fault at `P`. Every destination bit retains its pre-instruction value. |
| Split store at `P-16`, with the second mapping read-only or unmapped | Memory fault at `P`. The 16 writable bytes before `P` retain their sentinels. |
| 256-bit `VMOVDQU` load and store at `0x0000_7fff_ffff_fff0` | Map the first 16 bytes read/write and leave the noncanonical half starting at `0x0000_8000_0000_0000` unmapped. Both operations report `GeneralProtection` under the BetterXen profile. The destination register and the mapped bytes retain their original values. |
| 256-bit `VMOVDQU` load from `[rsp]` with `rsp = 0x0000_8000_0000_0000` | The BetterXen profile reports `GeneralProtection` and preserves the destination. Intel's Type 4 rules specify `#SS(0)` for this SS-based operand. |
| Future `VMOVDQA` at aligned and misaligned addresses | An aligned, permitted span transfers the same bytes. A span misaligned by one byte produces `GeneralProtection` and has no data effects. Run at both widths. |
| Future masked dword load from `P-16` | With only lanes 0–3 enabled, load four words and zero lanes 4–7 even if the mapping at `P` is absent. Enabling lane 4 faults at `P`. An all-zero mask reads no lanes and returns an all-zero destination. |
| Future masked dword store to the same split span | With only lanes 0–3 enabled, write those four words; disabled lanes neither access the second mapping nor alter any bytes there when it is mapped. Enabling lane 4 with its mapping absent faults at `P`. |
| Reserved move `vvvv`, reserved VEX map, prohibited prefix before VEX | Illegal instruction at the offending instruction's RIP. No destination or memory update. |
| Equivalent legal `C5` and `C4` register forms, including both `W` values for `WIG` | Equal results and state, with RIP advancement matching each encoding length. Include a high ModRM.reg destination and high `vvvv` source. |
| Budget of two steps | Stop at `VPADDD` (`entry + 0xA` with the encoding shown below) with both inputs loaded, YMM3 and output untouched. Resume with seven steps and reach the same final state as a nine-step run. Repeat with a warm DBT cache. |

For faults, place a completed instruction before the failing instruction.
Assert that only the earlier instruction retires and its effects persist.
Check snapshots of the full destination or nearby memory as appropriate.

## Build provenance

Use native Apple Silicon LLVM tools to assemble the sequence as an x86-64
object, for example with `xcrun clang -arch x86_64 -c avx-fixture.s`.
Verify the resolved Clang and `llvm-objdump` executables support ARM64 before
running them. Record their versions and the output of disassembling the
object. Apple Clang 21.0.0 emits `C4 C1 7E 6F 08` for the first load and
`C5 F5 FE DA` for `VPADDD` at offset `0xA`. Keep generated sources and
objects under ignored `artifacts/`.
The test harness consumes only the object's verified instruction bytes
as guest data. Reserved encodings can be authored separately from Intel's
bit-field tables, since an assembler may reject them. Use source-derived
lane values and checked-memory expectations for the test oracle.
