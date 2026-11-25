# Proposed AVX and AVX2 guest contract

**Historical native record.** Plans and results below retain their original candidate scope. Native development is paused under [ADR 0005](../adr/0005-crossover-hybrid-direction.md); follow the [hybrid architecture](../architecture.md) for active priorities.

[Issue #299](https://github.com/MacosPorters/betterxen/issues/299) defines a
reviewable first step for Advanced Vector Extensions (AVX) in the x86-64 guest.
The current [instruction registry](instruction-registry.md) rejects VEX prefixes.
[`CpuState`](../../crates/loader/src/cpu.rs) holds 16 XMM registers and MXCSR,
the SIMD floating-point control and status register. The synthetic `CPUID`
advertises basic leaves through 1 and reports no AVX or extended-state support.

The architectural expectations come from the public Intel Software Developer's
Manual and apply to BetterXen's synthetic x86-64 guest profile. Which AVX and
AVX2 features Xbox Series X|S CPUs expose, and which extended register state
the console OS enables for applications, remain open research questions.
Resolving them requires public platform documentation or authorized hardware
evidence.

## Architectural state and encoding

In 64-bit mode, XMM0–XMM15 alias bits 127:0 of YMM0–YMM15. A legacy SSE
instruction that writes an XMM destination preserves bits 255:128. For the
packed VEX.128 forms in this proposal, writing an XMM destination clears that
register's upper 128 bits. A VEX.256 register destination replaces all 256 bits.
A store to memory leaves its source vector register intact. The proposed moves
and integer add leave RFLAGS and MXCSR unchanged. [Intel SDM Vol. 1, §14.8,
Accessing YMM Registers][sdm-ymm]. [Intel SDM Vol. 2B, MOVDQU and PADDD][sdm-2b].

The shared [decoder](../../crates/loader/src/x86_decode.rs) should retain the
prefix family, opcode map, mandatory prefix, operand width and register
extensions until it selects the form. In 64-bit mode the VEX lead byte
identifies the prefix length. The table lists each payload byte's fields
from bit 7 to bit 0.

| Lead byte | Payload byte 1 | Payload byte 2 | Implied fields |
| --- | --- | --- | --- |
| `C5` | `~R ~vvvv L pp` | — | Map `0F`, `W=0`, logical `X=B=0` |
| `C4` | `~R ~X ~B mmmmm` | `W ~vvvv L pp` | Map selected by `mmmmm` |

The tilde indicates an inverted encoding. `R` extends ModRM.reg, while `X`
and `B` extend the applicable index and r/m or base fields. `vvvv` supplies
the first source of `VPADDD`. For `VMOVDQU` its encoded bits must be `1111b`.
`L=0` selects 128 bits and `L=1` selects 256 bits for these forms. The three
standard VEX maps are `00001b` (`0F`), `00010b` (`0F 38`) and `00011b`
(`0F 3A`). `pp` values 0 through 3 represent no mandatory prefix, `66`, `F3`
and `F2`. Both initial opcodes are in `0F`, with `W` ignored (`WIG`). Decode
both valid prefix lengths for them. Reject reserved maps and encodings,
including a physical `LOCK`, `66`, `F2`, `F3` or REX prefix before VEX, through
the existing illegal-instruction path. Legal forms outside the implemented
subset also stop explicitly. Keep the existing register and effective-address
rules and 15-byte fetch limit. [Intel SDM Vol. 2A, §2.3, VEX encoding][sdm-2a].
[Intel SDM Vol. 2B, MOVDQU and PADDD][sdm-2b].

## First implementation increments

1. Append one upper 128-bit half per vector register to `CpuState`, retaining
   the existing XMM storage as the low halves. Add layout assertions for the
   offsets the dynamic binary translator (DBT) uses. Initialize both halves
   in a fresh guest thread and carry them with the thread state when switching
   or resuming. Add the shared VEX decoder and encoding controls before execution.
2. Implement `VMOVDQU` (128 and 256 bits, `F3.0F`, `6F /r` and `7F /r`,
   register and checked memory forms) and `VPADDD` (128 and 256 bits,
   `66.0F`, `FE /r`, register sources initially). The instruction entries
   assign both move widths and 128-bit `VPADDD` to AVX, and 256-bit `VPADDD`
   to AVX2. Read all sources before publishing the destination, including
   when the destination aliases a source. Compare both engines with the
   [original fixture specification](../../tests/compat_fixtures/avx/README.md).
3. After shared semantics pass, lower register moves and `VPADDD` to native
   ARM64 vector operations on one or two 128-bit halves. Each `VPADDD` half
   is four independent 32-bit additions modulo 2³². Use the checked-memory
   helper for loads and stores. Check the DBT's fallback counter so a run
   through that helper establishes only helper coverage for those forms.

Stage memory-source `VPADDD` and aligned moves separately. Reject masked
moves, gathers, cross-half permutations, floating-point forms, FMA, EVEX
and guest extended-state instructions in the initial slice. AMD64 bytes
remain guest data. All host execution uses native ARM64 code or checked
Rust helpers.

## Memory, faults and instruction budget

`VMOVDQU` permits unaligned operands. Check the complete 16- or 32-byte
guest span before publishing a load result or writing a store. A split
span across adjacent permitted mappings succeeds. A missing or protected
mapping produces the existing guest memory fault at the first inaccessible
byte. For a later `VMOVDQA` form, a memory operand must be aligned to its
width (16 or 32 bytes), with a misaligned operand raising
`GeneralProtection`. [Intel SDM Vol. 2B, MOVDQU and MOVDQA][sdm-2b].

Intel's Type 4 rules for `VMOVDQU` specify `#SS(0)` when a memory reference
through the stack segment (SS) has a noncanonical address and `#GP(0)`
for other noncanonical references. The initial BetterXen profile uses the
[existing 48-bit guest address check](../../crates/loader/src/x86_exec.rs)
and reports `GeneralProtection` for either segment case, including a span
whose first byte is canonical and whose last byte is noncanonical. Check
canonicality before guest memory lookup and before committing the move.
Distinct stack-segment exception reporting remains outside this profile.
[Intel SDM Vol. 2A, Table 2-21, Type 4 exceptions][sdm-2a].

Before adding masked moves, use a separate per-lane access contract. For
`VPMASKMOVD`, only lanes with the high mask bit set may access memory or
report a memory fault. A clear bit zeroes a load lane and leaves a store
lane untouched. A test with lane 4 on an unmapped page must pass with that
lane disabled and fault with it enabled. A full-width span check would
incorrectly reject the disabled case. Specify store commit behavior before
admitting that form. [Intel SDM Vol. 2C, VPMASKMOV][sdm-2c].

Require interpreter and DBT parity for the exit reason, fault address when
reported, RIP, instruction count and complete guest register and memory
state. A rejected move or add has no partial destination update and does
not retire. A successful 256-bit instruction consumes one guest step even
when the DBT emits two halves. With an exhausted budget, stop at the next
guest RIP before decoding or executing that instruction. Check cold and warm
DBT paths and resume in the middle of the same sequence.

## Feature exposure

Run the initial forms only in explicitly enabled synthetic test cases.
Keep normal guest `CPUID` and extended-state behavior at the current baseline.
Before guest-visible AVX support, the guest OS model needs coherent reporting
for `CPUID.1:ECX` bits 26 (XSAVE), 27 (OSXSAVE) and 28 (AVX), and `XGETBV`
with `ECX=0` must report both XMM and YMM state enabled in `XCR0[2:1]`. The
extended-state layout reported by `CPUID.0Dh`, save and restore behavior,
and per-thread ownership must agree. Verify the exception behavior for
disabled state and invalid `XGETBV` selectors before exposing the facility.
AVX2 separately requires `CPUID.(7,0):EBX[5]` and the same OS state. Gate
each advertised feature on its full instruction and state contract,
with probes for enabled and disabled combinations. [Intel SDM Vol. 1,
§14.3, AVX detection][sdm-avx]. [Intel SDM Vol. 1, §14.7.1, AVX2 detection][sdm-avx2].
[Intel SDM Vol. 2A, CPUID][sdm-2a]. [Intel SDM Vol. 2D, XGETBV, XSAVE and XRSTOR][sdm-2d].

[sdm-ymm]: https://cdrdv2-public.intel.com/843827/253665-sdm-vol-1-dec-24.pdf#page=376
[sdm-avx]: https://cdrdv2-public.intel.com/843827/253665-sdm-vol-1-dec-24.pdf#page=359
[sdm-avx2]: https://cdrdv2-public.intel.com/843827/253665-sdm-vol-1-dec-24.pdf#page=375
[sdm-2a]: https://cdrdv2-public.intel.com/929353/253666-093-sdm-vol-2a.pdf
[sdm-2b]: https://cdrdv2-public.intel.com/929354/253667-093-sdm-vol-2b.pdf
[sdm-2c]: https://cdrdv2-public.intel.com/929355/326018-093-sdm-vol-2c.pdf
[sdm-2d]: https://cdrdv2-public.intel.com/929356/334569-093-sdm-vol-2d.pdf
