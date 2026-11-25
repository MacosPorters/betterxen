# Unwind fixture specification

This is the original fixture that a future implementation of the [bounded AMD64 unwind profile](../../../docs/cpu/guest-unwind.md) must pass (#300). **No unwinder exists yet.** This page fixes the program, its build, its unwind metadata and the expected results before any implementation, so they are not taken from production output.

## Program

Original x86-64 assembly with GNU `.seh_*` directives, so each prolog is exactly as written:

- `start` reserves the 32-byte home area its callee may use, plus 8 bytes that align RSP. It sets sentinel values in the non-volatile registers RBX, RBP, RSI, RDI, R12 and XMM6, then calls `f1`.
- `f1` has a frame pointer: it pushes RBP and RBX, makes a small allocation and sets RBP. After `f2` returns it runs one body instruction, then a `lea rsp` epilog.
- `f2` pushes RSI and RDI, makes a large allocation and saves R12 and XMM6 with moves.
- `f3` is a leaf with no `RUNTIME_FUNCTION`. It stops at `hlt`.

Each function overwrites the registers it saved before calling deeper, so a correct unwind has to restore values that differ from the current ones. A later PR adds the file as `tests/compat_fixtures/unwind/unwind.S`, byte for byte as below.

```asm
    .intel_syntax noprefix
    .text
    .globl start
    .def start; .scl 2; .type 32; .endef
    .seh_proc start
start:
    sub rsp, 0x28
    .seh_stackalloc 0x28
    .seh_endprologue
    movabs rbx, 0xB0B0B0B0B0B0B0B0
    movabs rbp, 0xB1B1B1B1B1B1B1B1
    movabs rsi, 0xB2B2B2B2B2B2B2B2
    movabs rdi, 0xB3B3B3B3B3B3B3B3
    movabs r12, 0xB4B4B4B4B4B4B4B4
    movabs rax, 0xC6C6C6C6C6C6C6C6
    movq xmm6, rax
    punpcklqdq xmm6, xmm6
    call f1
ret_start:
    hlt
    .seh_endproc

    .def f1; .scl 2; .type 32; .endef
    .seh_proc f1
f1:
    push rbp
    .seh_pushreg rbp
    push rbx
    .seh_pushreg rbx
    sub rsp, 0x28
    .seh_stackalloc 0x28
    lea rbp, [rsp + 0x20]
    .seh_setframe rbp, 0x20
    .seh_endprologue
    movabs rbx, 0x1111111111111111
    call f2
ret_f1:
    xor eax, eax
f1_epilog:
    lea rsp, [rbp + 0x8]
    pop rbx
    pop rbp
    ret
    .seh_endproc

    .def f2; .scl 2; .type 32; .endef
    .seh_proc f2
f2:
    push rsi
    .seh_pushreg rsi
    push rdi
    .seh_pushreg rdi
    sub rsp, 0x208
    .seh_stackalloc 0x208
    mov [rsp + 0x200], r12
    .seh_savereg r12, 0x200
    movaps [rsp + 0x1e0], xmm6
    .seh_savexmm xmm6, 0x1e0
    .seh_endprologue
    movabs rsi, 0x2222222222222222
    movabs rdi, 0x3333333333333333
    movabs r12, 0x4444444444444444
    movabs rax, 0x6666666666666666
    movq xmm6, rax
    punpcklqdq xmm6, xmm6
    call f3
ret_f2:
    movaps xmm6, [rsp + 0x1e0]
    mov r12, [rsp + 0x200]
    add rsp, 0x208
    pop rdi
    pop rsi
    ret
    .seh_endproc

    .globl f3
f3:
    hlt
    ret
```

## Build

The tools are native arm64: `file` reports `Mach-O 64-bit executable arm64` for `x86_64-w64-mingw32-gcc`. Measured with mingw-w64 14.0.0 (GCC 16.2.0, GNU ld 2.47) from Homebrew:

```sh
x86_64-w64-mingw32-gcc -c unwind.S -o unwind.o
x86_64-w64-mingw32-ld -e start --image-base 0x140000000 -o unwind.exe unwind.o
```

No C library or runtime is linked. Generated objects and listings belong under the ignored `artifacts/`.

## Unwind metadata (independent decode)

`llvm-readobj --unwind unwind.exe` (Homebrew LLVM 23.1.5) decodes the image as follows. It is a decoder independent of BetterXen, and a future parser must agree with it entry by entry. Its exact output is under [Recorded decoder output](#recorded-decoder-output).

| Function | RVA range | Version, flags | Prolog | Frame | Codes (slots) |
| --- | --- | --- | --- | --- | --- |
| `start` | 0x1000–0x104F | 1, 0 | 4 | — | `0x04 ALLOC_SMALL 40` (1) |
| `f1` | 0x104F–0x1072 | 1, 0 | 11 | RBP, offset 2 (32 bytes) | `0x0B SET_FPREG RBP+0x20`, `0x06 ALLOC_SMALL 40`, `0x02 PUSH_NONVOL RBX`, `0x01 PUSH_NONVOL RBP` (4) |
| `f2` | 0x1072–0x10DB | 1, 0 | 25 | — | `0x19 SAVE_XMM128 XMM6 0x1E0`, `0x11 SAVE_NONVOL R12 0x200`, `0x09 ALLOC_LARGE 520`, `0x02 PUSH_NONVOL RDI`, `0x01 PUSH_NONVOL RSI` (8) |
| `f3` | 0x10DB– | no entry (leaf) | | | |

Return sites, as RVAs:

| Return site | RVA |
| --- | --- |
| `ret_start` | 0x104E |
| `ret_f1` | 0x1069 |
| `ret_f2` | 0x10C1 |

`ret_f1` is `xor eax, eax`, a body instruction. `f1`'s epilog starts at RVA 0x106B with `lea rsp, [rbp+8]` (`48 8D 65 08`), then `pop rbx`, `pop rbp` and `ret`. `f2`'s epilog starts at RVA 0x10D1 with `add rsp, 0x208`, then `pop rdi`, `pop rsi` and `ret` (`5F 5E C3` at 0x10D8).

## Expected results

**Entry requirement.** The harness enters `start` with RSP ≡ 8 (mod 16), as after a `call`, and with at least 1 KiB of mapped, writable stack below it. With any other alignment `f2`'s `movaps` faults with #GP, and the case is invalid rather than a refusal.

Let *S0* be RSP at `start`'s `call f1`, before the return address is pushed. *S0* is 16-aligned, because `start` subtracts 40 from its entry RSP. The 32 bytes at *S0* to *S0* + 31 are `f1`'s home area; nothing in the fixture writes them. *B* is the image base. Every value below follows by hand from the instructions and Microsoft's unwind-code definitions. No BetterXen code produced them.

Stack contents while `f3` is at its `hlt`:

| Address | Holds |
| --- | --- |
| S0 − 8 | *B* + 0x104E (return into `start`) |
| S0 − 16, S0 − 24 | `start`'s RBP, RBX (0xB1…B1, 0xB0…B0) |
| RBP | S0 − 32 (`f1`'s frame pointer) |
| S0 − 72 | *B* + 0x1069 (return into `f1`) |
| S0 − 80, S0 − 88 | `start`'s RSI, RDI (0xB2…B2, 0xB3…B3) |
| S0 − 96 | `start`'s R12 (0xB4…B4), saved by `f2` |
| S0 − 128 | `start`'s XMM6 (16 bytes of 0xC6), saved by `f2` |
| S0 − 616 | *B* + 0x10C1 (return into `f2`) |

### Walk from `f3`'s `hlt` (body and leaf cases)

At the `hlt`, RIP = *B* + 0x10DB and RSP = S0 − 616. `f2` has set RSI = 0x22…22, RDI = 0x33…33, R12 = 0x44…44 and XMM6 = 16 bytes of 0x66. `f1` has set RBX = 0x11…11.

| Step | Rule | RIP after | RSP after | Restored |
| --- | --- | --- | --- | --- |
| frame 0, `f3` | leaf: no entry; ends after the return | *B* + 0x10C1 | S0 − 608 | — |
| frame 1, `f2` body (`movaps` at 0x10C1 is not an epilog) | Base = S0 − 608; undo all 5 codes, then return | *B* + 0x1069 | S0 − 64 | XMM6 ← [S0 − 128], R12 ← [S0 − 96], RDI ← [S0 − 88], RSI ← [S0 − 80] |
| frame 2, `f1` body (`xor` at 0x1069 is not an epilog) | Base = RBP − 32 = S0 − 64; undo all 4 codes, then return | *B* + 0x104E | S0 | RBX ← [S0 − 24], RBP ← [S0 − 16] |

After frame 2:

- RBX, RBP, RSI, RDI, R12 and XMM6 hold exactly `start`'s sentinels.
- Volatile registers are not part of the result.
- The walk continues into `start` only if the harness provides a return address for it. Otherwise it stops at the configured depth.

### Prolog case

Run with a step budget that stops `f2` after its `sub rsp, 0x208`, with RIP = *B* + 0x107B (prolog offset 9) and RSP = S0 − 608.

- Undo only the codes with `CodeOffset` ≤ 9: ALLOC_LARGE, PUSH RDI and PUSH RSI.
- Result: RIP = *B* + 0x1069, RSP = S0 − 64, and RSI and RDI hold `start`'s values.
- R12 and XMM6 are not loaded from the stack, because their saves had not executed. They keep their current values, which are still `start`'s.

### Epilog cases

Both are terminal: the simulated `ret` produces the caller's RIP and RSP, and no unwind code is applied.

**`add` form.** Stop `f2` after `add rsp, 0x208`, with RIP = *B* + 0x10D8 and RSP = S0 − 88.

- The bytes `5F 5E C3` (`pop rdi; pop rsi; ret`) match the end of a legal epilog, so they are simulated: RDI ← [S0 − 88], RSI ← [S0 − 80], RIP ← [S0 − 72] = *B* + 0x1069, RSP = S0 − 64.

**`lea` form.** Stop `f1` at the start of its epilog, after `xor eax, eax`, with RIP = *B* + 0x106B, RSP = S0 − 64 and RBP = S0 − 32.

- `48 8D 65 08 5B 5D C3` is a whole legal epilog: RSP ← RBP + 8 = S0 − 24, RBX ← [S0 − 24], RBP ← [S0 − 16], RIP ← [S0 − 8] = *B* + 0x104E, RSP = S0.
- RBX and RBP hold `start`'s sentinels.

## Coverage

The profile specifies more than this fixture exercises. Only what is listed as covered may be called verified by it.

| Covered here | Specified, not covered by a positive case |
| --- | --- |
| `UWOP_PUSH_NONVOL`, `UWOP_ALLOC_SMALL`, `UWOP_ALLOC_LARGE` OpInfo 0, `UWOP_SET_FPREG`, `UWOP_SAVE_NONVOL`, `UWOP_SAVE_XMM128` | `UWOP_ALLOC_LARGE` OpInfo 1, `UWOP_SAVE_NONVOL_FAR`, `UWOP_SAVE_XMM128_FAR`, `UWOP_PUSH_MACHFRAME` |
| leaf, body, prolog, `add` and `lea` epilogs | `jmp` tail-call epilogs, epilogs with `41 5x` pops |
| a three-frame walk | chained info (only its refusals are covered), the handler flags |

Each uncovered item needs its own original positive fixture before an implementation of it is accepted.

## Negative variants

Each variant is one change to a copy of `unwind.exe` or to the starting context. Each must be refused with a reason, and the input context must be unchanged.

| Variant | Change | Required result |
| --- | --- | --- |
| Truncated table | Exception Table `Size` not a multiple of 12 | table refused |
| Out-of-range entry | `f2`'s `EndAddress` beyond `SizeOfImage` | table refused |
| Overlap | `f1`'s `EndAddress` set to 0x1080 | table refused |
| Bad version | `f2`'s `UNWIND_INFO` version set to 2 | `f2` refused |
| Count overflow | `f2`'s `CountOfCodes` 9, which needs slots past the data | `f2` refused |
| Undocumented op | an unwind op of 6 | `f2` refused |
| Bad ALLOC_LARGE | OpInfo 2 | `f2` refused |
| Frame register RSP | `f1`'s `FrameRegister` 4 | `f1` refused |
| Unreadable stack | starting RSP inside an unmapped page | unwind refused at the first read |
| Chain cycle | a chained entry whose parent is itself | refused, cycle |
| Chain depth | 33 chained entries | refused, depth |
| Chained push | a chained entry with a `PUSH_NONVOL` | refused |
| No progress | a stack slot that makes the caller's RSP not greater than the callee's | walk refused at that frame, earlier frames kept |

## Recorded decoder output

`llvm-readobj --unwind unwind.exe`, Homebrew LLVM 23.1.5, on the image built as above:

```text

File: unwind.exe
Format: COFF-x86-64
Arch: x86_64
AddressSize: 64bit
UnwindInformation [
  RuntimeFunction {
    StartAddress: start (0x140001000)
    EndAddress: f1 (0x14000104F)
    UnwindInfoAddress: .xdata (0x140004000)
    UnwindInfo {
      Version: 1
      Flags [ (0x0)
      ]
      PrologSize: 4
      FrameRegister: -
      FrameOffset: -
      UnwindCodeCount: 1
      UnwindCodes [
        0x04: ALLOC_SMALL size=40
      ]
    }
  }
  RuntimeFunction {
    StartAddress: f1 (0x14000104F)
    EndAddress: f2 (0x140001072)
    UnwindInfoAddress: (0x140004008)
    UnwindInfo {
      Version: 1
      Flags [ (0x0)
      ]
      PrologSize: 11
      FrameRegister: RBP (0x5)
      FrameOffset: 0x2
      UnwindCodeCount: 4
      UnwindCodes [
        0x0B: SET_FPREG reg=RBP, offset=0x20
        0x06: ALLOC_SMALL size=40
        0x02: PUSH_NONVOL reg=RBX
        0x01: PUSH_NONVOL reg=RBP
      ]
    }
  }
  RuntimeFunction {
    StartAddress: f2 (0x140001072)
    EndAddress: f3 (0x1400010DB)
    UnwindInfoAddress: (0x140004014)
    UnwindInfo {
      Version: 1
      Flags [ (0x0)
      ]
      PrologSize: 25
      FrameRegister: -
      FrameOffset: -
      UnwindCodeCount: 8
      UnwindCodes [
        0x19: SAVE_XMM128 reg=XMM6, offset=0x1E0
        0x11: SAVE_NONVOL reg=R12, offset=0x200
        0x09: ALLOC_LARGE size=520
        0x02: PUSH_NONVOL reg=RDI
        0x01: PUSH_NONVOL reg=RSI
      ]
    }
  }
]
```
