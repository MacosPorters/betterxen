# Wave32 vector select, carry and borrow

**Preserved native research.** This page governs existing synthetic fixtures and their regression checks. It does not define CrossOver compatibility or authorize native expansion; follow the [hybrid architecture](../architecture.md) for active work.

The gfx1030 compute fixture ABI supports four 32-bit vector two-operand
instruction (VOP2) forms that read the vector condition code (VCC).
In Wave32, bit `n` of `VCC_LO` supplies lane `n`'s input `c`.
The AMD RDNA 2 guide describes the operations in section 12.7, the VOP2 word
in section 13.3.1, and the execution and condition masks in sections 3.3 and 3.9.

| Instruction | Active lane destination | Active lane VCC output |
| --- | --- | --- |
| `v_cndmask_b32_e32` | S1 if c is set, S0 otherwise | Preserved |
| `v_add_co_ci_u32_e32` | Low 32 bits of S0 + S1 + c | Exact sum reaches 2³² |
| `v_sub_co_ci_u32_e32` | Low 32 bits of S0 − S1 − c | S1 + c exceeds S0 |
| `v_subrev_co_ci_u32_e32` | Low 32 bits of S1 − S0 − c | S0 + c exceeds S1 |

Section 12.7 prints a reverse-subtract flag expression that conflicts with its
prose and destination equation. Xenora uses unsigned borrow from the stated
reverse subtraction, consistent with section 12.12's `v_subrev_co_u32` when
the incoming borrow is zero.

Inactive lanes retain the old vector destination (VDST). Section 3.9
describes a fully written VCC mask and defines carry-out in terms of executing
lanes. Under that reading, inactive lanes contribute zero to a carry or borrow
output. Section 3.3 says scalar-register-writing vector ALU instructions
still execute under empty EXEC, so the carry output is a zero mask.
The synthetic ABI also zeros
absent lanes in a final short wave when it constructs a new mask.
The scalar condition code (SCC) and EXEC are preserved by these forms.

The general SRC0 subset includes vector general-purpose registers (VGPRs),
scalar registers s0–s105, `vcc_lo`, inline integers and one literal word.
VSRC1 and VDST are VGPRs. Translated scalar sources must be known or supplied
by a translated VCC result. The pinned XML lists the VCC input on all four
forms and an additional VCC output on the three arithmetic forms. VOP3
encodings with explicit scalar conditions and VOP3B carry-out-only forms
are outside this subset.

The Metal lowering reuses the compare mask's replay of the current wave's
input lanes to form carry and borrow masks. The original `vector-vcc.s`
fixture chains all four operations. LLVM checks encoding, and the guide-derived
oracle checks both CPU interpreters. `vop2_vcc_matches_guide_on_native_metal` passes on a local
Apple M5 under API and shader validation; independent acceptance is pending. The expectations are derived from the public AMD specification (class B).
