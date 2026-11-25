# Bounded AMD64 unwind for guest code

This page specifies a bounded profile for reading x64 unwind data from a guest PE32+ image and virtually unwinding guest stack frames. It is research for [#300](https://github.com/MacosPorters/betterxen/issues/300): **nothing here is implemented**. A parser or unwinder needs its own issue and agreed file list. The fixture that a future implementation must pass is specified in [`tests/compat_fixtures/unwind/README.md`](../../tests/compat_fixtures/unwind/README.md).

The profile follows Microsoft's public documentation:

- [x64 exception handling](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64) for `RUNTIME_FUNCTION`, `UNWIND_INFO`, `UNWIND_CODE`, chained info and the unwind procedure;
- [x64 prolog and epilog](https://learn.microsoft.com/en-us/cpp/build/prolog-and-epilog) for the legal epilog forms;
- [PE format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format), "Optional Header Data Directories", for the Exception Table directory.

These describe Windows on desktop x64. Whether Xbox titles use the same metadata is unknown, and nothing here establishes Xbox behaviour.

## What exists today

On main `67b8cff`:

- The loader does not read the Exception Table data directory, so `.pdata` is ignored.
- `ucrtbase.dll!__C_specific_handler` is the #240 stub. It stops the guest with the runtime-limit fault and dispatches nothing. That trap stays until a real path exists.
- A guest fault ends the run with its `ExitReason`. Nothing is delivered to the guest.
- `probe.rs` only groups imports from `rtlsupport` and `errorhandling` DLLs and `__CxxFrameHandler*` under "exceptions and unwind".

## Five separate capabilities

They build on each other, and each must be accepted on its own. Passing one says nothing about the next.

| Capability | What it does | In this profile |
| --- | --- | --- |
| Function lookup | Find the `RUNTIME_FUNCTION` covering a guest RIP | Yes |
| Virtual unwind | Compute the caller's RIP, RSP and non-volatile registers from one frame | Yes |
| Stack walk | Repeat the virtual unwind, read-only, for diagnostics (#255) | Yes, bounded |
| Guest fault delivery | Turn a guest fault into an exception record and context and start dispatch | No |
| Handler dispatch | Call language-specific handlers (`__C_specific_handler`), and C++ exceptions (`__CxxFrameHandler*`) | No. The scope-table format is not on the pages reviewed and is unknown here |

## Locating the table

1. **The directory.** The Exception Table is the fourth entry of the optional header's data directories, at offset 136 in a PE32+ header. Its `VirtualAddress` is an RVA and `Size` is in bytes.
   - The entry exists only if `NumberOfRvaAndSizes` is at least 4. Otherwise the image has no table, which is not an error.
   - The PE page warns not to assume the RVA starts a section. The profile requires only that the range lies inside the mapped image.
2. **Entries.** Each entry is a `RUNTIME_FUNCTION` of three 32-bit image-relative fields: `BeginAddress`, `EndAddress`, `UnwindData` (12 bytes, DWORD-aligned). Microsoft states the entries are sorted. The profile also requires:

   | Rule | On failure |
   | --- | --- |
   | `Size` is a multiple of 12 and at most 65,536 entries | refuse the table |
   | Every `BeginAddress < EndAddress ≤ SizeOfImage` | refuse the table |
   | Entries strictly increasing and **non-overlapping** (`EndAddress[i] ≤ BeginAddress[i+1]`). Overlap is not stated by Microsoft; it is a profile restriction | refuse the table |
   | `UnwindData` is DWORD-aligned and its `UNWIND_INFO`, including codes and trailing data, lies inside the image | refuse that entry |

3. **Lookup.** A binary search on `BeginAddress ≤ RIP − ImageBase < EndAddress`. No match means a leaf function (see [Virtual unwind](#virtual-unwind)).

## UNWIND_INFO profile

Byte 0 holds `Version` (3 bits) and `Flags` (5 bits); byte 1 `SizeOfProlog`; byte 2 `CountOfCodes`; byte 3 `FrameRegister` (4 bits) and `FrameOffset` (4 bits).

Microsoft gives these as a C bitfield declaration and does not state bit positions. The profile takes `Version` and `FrameRegister` from the **low** bits of their bytes. That is how the declared fields are allocated on x64 Windows compilers, and it matches `llvm-readobj --unwind` on the fixture. An implementation must test it against that independent decoder, not assume it.

| Field | Accepted | Otherwise |
| --- | --- | --- |
| `Version` | 1 only. Microsoft documents only version 1. Version 2 and its `UWOP_EPILOG` code, and the "Unwind V3" preview, are not on the pages reviewed | refuse |
| `Flags` | `0`, `UNW_FLAG_EHANDLER` (1), `UNW_FLAG_UHANDLER` (2), both (3), or `UNW_FLAG_CHAININFO` (4) alone | refuse |
| `CountOfCodes` | the slots used by the codes fill exactly `CountOfCodes`; the array is padded to an even slot count | refuse |
| `FrameRegister` | 0, or a non-volatile integer register: RBX 3, RBP 5, RSI 6, RDI 7, R12–R15 12–15. RSP (4) is refused. Microsoft says "nonvolatile register"; the exact set is the Windows x64 calling convention's | refuse |
| `FrameOffset` | 0–15, meaning 0–240 bytes; must be 0 when `FrameRegister` is 0 | refuse |

Codes appear in **descending** `CodeOffset` order, and each `CodeOffset` must be at most `SizeOfProlog`. A violation refuses the entry. The handler RVA and handler data after the codes are read only to check that they lie inside the image. They are never called in this profile.

### Unwind codes

Integer registers use OpInfo 0 = RAX, 1 = RCX, 2 = RDX, 3 = RBX, 4 = RSP, 5 = RBP, 6 = RSI, 7 = RDI, 8–15 = R8–R15. *Base* is the fixed-allocation base: RSP when `FrameRegister` is 0, otherwise FP − 16 × `FrameOffset`.

| Op | Name | Slots | Undo effect |
| --- | --- | --- | --- |
| 0 | `UWOP_PUSH_NONVOL` | 1 | reg ← [RSP]; RSP += 8 |
| 1 | `UWOP_ALLOC_LARGE` | 2 (OpInfo 0: next slot × 8) or 3 (OpInfo 1: next two slots, unscaled) | RSP += size; any other OpInfo is refused |
| 2 | `UWOP_ALLOC_SMALL` | 1 | RSP += OpInfo × 8 + 8 |
| 3 | `UWOP_SET_FPREG` | 1 | RSP ← FP − 16 × `FrameOffset`; refused if `FrameRegister` is 0 |
| 4 | `UWOP_SAVE_NONVOL` | 2 (next slot × 8) | reg ← [Base + offset] |
| 5 | `UWOP_SAVE_NONVOL_FAR` | 3 (next two slots) | reg ← [Base + offset] |
| 8 | `UWOP_SAVE_XMM128` | 2 (next slot × 16) | xmmN ← 16 bytes at [Base + offset], 16-aligned |
| 9 | `UWOP_SAVE_XMM128_FAR` | 3 (next two slots) | as above |
| 10 | `UWOP_PUSH_MACHFRAME` | 1 | OpInfo 0: RIP ← [RSP], RSP ← [RSP + 24]. OpInfo 1: RIP ← [RSP + 8], RSP ← [RSP + 32]. Ends the frame: no return address is popped |
| 6, 7, 11–15 | — | — | refuse (undocumented) |

Microsoft's own text is inconsistent on `UWOP_ALLOC_LARGE`: the C enum comment says "next 2 slots", while the prose gives OpInfo 0 one extra slot and OpInfo 1 two. The profile follows the prose. `llvm-readobj` decodes the fixture's OpInfo 0 allocation of 520 as two slots in total.

## Virtual unwind

The input is a guest context (RIP, RSP, the integer registers and XMM6–XMM15) and the mapped image. The output is the caller's context, or a refusal that leaves the input context unchanged. The computation works on a copy. Every stack read must lie in mapped, readable guest memory, or the whole unwind is refused.

1. **No covering entry** (a leaf function): RIP ← [RSP], RSP += 8.
2. **Epilog.** Microsoft's epilog is exactly `add rsp, imm` or `lea rsp, [FP + disp]`, then zero or more 8-byte pops, then `ret` or a `jmp` whose ModRM `mod` is 00. Nothing else may be scheduled in it. The profile recognizes these encodings: `48 83 C4 ib`, `48 81 C4 id`, `48 8D 65 ib` and its other FP encodings, `5x` and `41 5x`, `C3`, `FF /4` with `mod` 00.
   - If the bytes from RIP to that `ret` or `jmp` form a trailing part of such a sequence, the remainder is simulated: the add or lea adjusts RSP, the pops restore registers, and `ret` or `jmp` takes RIP ← [RSP], RSP += 8.
   - A `jmp` in an epilog is a tail call, so for unwinding it behaves like `ret`.
   - Bytes that only partly match are not an epilog.
3. **Prolog**, when RIP − `BeginAddress` ≤ `SizeOfProlog`: undo only codes whose `CodeOffset` ≤ RIP − `BeginAddress`, in array order. Those are the operations that have executed.
4. **Body**: undo every code, in array order.
5. **Base.** It is fixed once, before undoing:
   - FP − 16 × `FrameOffset` if a `UWOP_SET_FPREG` is among the codes being undone;
   - otherwise RSP.

   FP is read before any code restores the frame register.
6. **Chained info** (`UNW_FLAG_CHAININFO`): after the codes, continue with the parent `RUNTIME_FUNCTION` that follows the padded array, undoing all its codes. The profile also requires:
   - Chained entries contain only `UWOP_SAVE_NONVOL`, `UWOP_SAVE_NONVOL_FAR`, `UWOP_SAVE_XMM128` and `UWOP_SAVE_XMM128_FAR`. Microsoft rules out pushes and extra fixed allocations in a chained grouping.
   - The handler flags are clear.
   - `FrameRegister` and `FrameOffset` equal the primary's.
   - A chain longer than 32 entries, or one that revisits an entry, is refused.
7. Unless `UWOP_PUSH_MACHFRAME` set them, RIP ← [RSP] and RSP += 8.

## Stack walk

A stack walk repeats the virtual unwind, read-only.

- **Progress.** The caller's RSP must be strictly greater than the callee's, or the walk is refused at that frame. This catches loops and corrupt stacks.
- **Stop conditions.** The walk stops at a configured depth (at most 256 frames), at a RIP outside every mapped executable image, or at a refusal.
- **Records.** Each completed frame is reported. A refusal is reported with the frame index and reason, and earlier frames are kept. For #255, a refusal is never shown as a complete walk.

## Out of scope and unknown

- Version 2 epilog codes and the V3 preview format: refused, not interpreted.
- Dynamically registered function tables (`RtlAddFunctionTable`, `RtlInstallFunctionTableCallback`). Guest code generation is not supported.
- `__chkstk` probes. Microsoft requires a probe for fixed allocations of a page or more; its two phrasings differ (more than 4096 bytes, or at least one page). Unwinding is unaffected, because the probe runs before `sub rsp`.
- Exception records, `RtlUnwindEx`-style target unwinding, handler dispatch, `__C_specific_handler` scope tables, C++ exceptions and vectored handlers.
- Anything about Xbox-specific unwind or exception behaviour.
