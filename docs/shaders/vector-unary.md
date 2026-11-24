# Wave32 vector unary integer operations and swap

Xenora translates six gfx1030 VOP1 integer forms for the synthetic Wave32 compute
ABI. The AMD RDNA 2 ISA Reference Guide defines the operations in section 12.8,
the encoding in section 13.3.2, and the execution mask (EXEC) in section 3.3.

| Form | Result in each active lane |
| --- | --- |
| `v_not_b32_e32` | Complement all 32 bits. |
| `v_bfrev_b32_e32` | Reverse the order of the 32 bits. |
| `v_ffbh_u32_e32` | Count leading zeros. Return all ones if the source is zero. |
| `v_ffbl_b32_e32` | Count trailing zeros. Return all ones if the source is zero. |
| `v_ffbh_i32_e32` | Count leading bits equal to the sign bit. Return all ones if all bits match. |
| `v_swap_b32` | Exchange the old values of two vector general-purpose registers (VGPRs). A register swapped with itself keeps its value. |

The unary forms accept a VGPR, scalar register `s0`–`s105`, `vcc_lo` (the low
half of the vector condition code), inline integer `-16`–`64`, or a literal
word as SRC0. Swap requires a VGPR as SRC0 because it writes both operands.
Both operands of swap must be initialized when EXEC is nonzero. The compute
ABI reserves `v0` for indexing and requires waiting for buffer loads before
using or overwriting their destination. Inactive lanes retain their old
values. The six forms leave EXEC, VCC and the scalar condition code (SCC)
unchanged.

The compiler emits per-thread Metal Shading Language for unary operations.
For swap it captures both old values in the intermediate representation before
updating either register binding. Under partial EXEC, each register receives
its own lane merge.

The [guide-derived oracle](../../tests/validation/tests/vop1_integer.rs) checks
the 752 accepted cases' outputs against its expected values on each CPU
interpreter and, through the translator, on native Metal (Apple M5, API and
shader validation on: 752 cases in 613 programs). A CPU test checks that the
compiler accepts every case and refuses each of the 610 rejected programs at
the same instruction offset as the interpreters. The
[LLVM-assembled fixture](../../fixtures/rdna2/vector-unary.s) exercises all
six forms with a partial EXEC swap. Independent acceptance of the
translator is pending. Xbox hardware agreement is unknown.
