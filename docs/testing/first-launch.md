# First compiled launch: acceptance protocol

This protocol decides when the first compiled guest application counts as launched. That means an original C program, built on Apple Silicon into an x86-64 PE, run by BetterXen's ARM64 translator and runtime, choosing its own graphics resources and translated shaders, rendering on Metal into a real app window, and closing and restarting cleanly. The parent tracker is [#311](https://github.com/MacosPorters/betterxen/issues/311). This page is the gate for [#321](https://github.com/MacosPorters/betterxen/issues/321).

It defines launchability for one original fixture. It is not a claim about playable Xbox software, commercial titles, Xbox hardware or performance.

**Status: protocol and expectations proposed; nothing has run.** The executable path does not exist yet, so every checkpoint reports `PENDING`. The expected values below were written before any implementation, by someone who is not implementing the launch path. They become acceptance criteria only after the fixture owners agree them in [#159](https://github.com/MacosPorters/betterxen/issues/159) and [#313](https://github.com/MacosPorters/betterxen/issues/313), and an R2 reviewer re-derives them (see [Reviews](#reviews)).

## Running the gate

```sh
scripts/test-launchable.sh              # check the expectations, then report each checkpoint
scripts/test-launchable.sh --self-test  # also prove a changed expected pixel is rejected
```

| Exit status | Meaning |
| --- | --- |
| 0 | Every checkpoint ran and passed |
| 1 | A checkpoint failed, or [`expected.json`](../../tests/launchable/expected.json) is inconsistent |
| 2 | Nothing failed, but at least one checkpoint could not run (`PENDING`) |

A pending checkpoint is never a pass. Each run writes `record.json` under `artifacts/launchable/`, which Git ignores. The record holds the revision and whether the tree was dirty, the host (machine, macOS version, chip), the command and every checkpoint's status and reason. When a prerequisite lands, its PR replaces that checkpoint's `PENDING` entry in the script with the real check. Nothing may report `PASS` without executing.

## Fixture contract (proposed)

The values live in [`tests/launchable/expected.json`](../../tests/launchable/expected.json). The script recomputes every pixel from the rules below and compares it with that file, so a typo in either place fails the gate.

- **Frame.** The existing fixture frame size from the C interface, `XENORA_FIXTURE_FRAME_WIDTH` × `XENORA_FIXTURE_FRAME_HEIGHT` = 4 × 3, RGBA8 unorm. Pixels are listed top row first, left to right. The coordinate origin is an assumption for #159 to confirm.
- **Sequence.** 60 frames per session: frames 0–29 in state A and 30–59 in state B. Sixty is a fixture criterion from #311, not a frame rate.
- **State A.** The guest selects a constant-colour pixel pipeline covering the frame. Its constants are 255/255, 128/255, 0/255 and 255/255, which convert to 8-bit unorm without rounding. Every pixel is `(255, 128, 0, 255)`.
- **State B.** The guest writes a 4 × 3 RGBA8 texture with texel (x, y) = `(64·x, 96·y, 255 − 64·x, 255)`. It then draws a full-frame quad through a textured pipeline it selects, with nearest filtering and clamp-to-edge. Pixel (x, y) is sampled at its centre, ((x + 0.5)/4, (y + 0.5)/3) in texture coordinates, which is the centre of texel (x, y), so pixel (x, y) equals texel (x, y):

  | y \ x | 0 | 1 | 2 | 3 |
  | --- | --- | --- | --- | --- |
  | 0 | 0, 0, 255, 255 | 64, 0, 191, 255 | 128, 0, 127, 255 | 192, 0, 63, 255 |
  | 1 | 0, 96, 255, 255 | 64, 96, 191, 255 | 128, 96, 127, 255 | 192, 96, 63, 255 |
  | 2 | 0, 192, 255, 255 | 64, 192, 191, 255 | 128, 192, 127, 255 | 192, 192, 63, 255 |

  Every B pixel's red channel is below 255, so A and B differ at every pixel. Changing either the texture or the selected pipeline changes the answer, which is what "guest-controlled" requires.
- **Exit.** The guest exits with status `0x4C4E4348` (ASCII `LNCH` read as a big-endian u32) after reporting 60 submitted frames. Any other status is a failure.
- **Restart.** After a clean close, a second session in the same app process repeats frames 0–59 from state A with the same pixels and exit status. Nothing carries over between sessions.

The sampling rule follows the public nearest-filtering definitions in the Direct3D 11 and Metal texture sampling documentation. The expected values deliberately avoid interpolated colours and partial coverage, so no rasterization or rounding choice can change them.

## Checkpoints

The checkpoints are separate: a later one never stands in for an earlier one.

| Checkpoint | Evidence required | Does not count | Waits on |
| --- | --- | --- | --- |
| Build and identity | Source hash, tool names and versions verified as native arm64 (`file`, `lipo -archs`), exact compile and link commands, PE facts (sections, imports, relocations), licences of every linked file | A build on an Intel or Rosetta tool; a downloaded or prebuilt binary | [#313](https://github.com/MacosPorters/betterxen/issues/313) |
| Admit and load | The file is admitted under the named built-in profile and mapped; an unadmitted copy of it is refused | Any relaxation of the exact-bytes gate outside [#212](https://github.com/MacosPorters/betterxen/issues/212) | [#317](https://github.com/MacosPorters/betterxen/issues/317) |
| Guest entry | Imports bound, DBT entry and ordered progress records ([#255](https://github.com/MacosPorters/betterxen/issues/255)) | Interpreter-only execution reported as translated | #317, #255 |
| First native frame | Frame 0 rendered by Metal from guest-selected pipeline and resources and read back, equal to state A | A CPU reference, a mock or a skipped GPU test | [#314](https://github.com/MacosPorters/betterxen/issues/314), [#315](https://github.com/MacosPorters/betterxen/issues/315), [#316](https://github.com/MacosPorters/betterxen/issues/316) |
| Presentation | The current session's frame visible in the app window, identified as that session's frame | Readback alone; a static preview; a host-drawn stand-in | [#320](https://github.com/MacosPorters/betterxen/issues/320) |
| Repeated frames | Frames 0–59 equal states A then B, each frame checked | Checking only the first and last frame | #316, #317 |
| Close and restart | Bounded stop, clean close, then an identical second session and the same exit status | A restart that hides a first-session failure | [#318](https://github.com/MacosPorters/betterxen/issues/318), #320 |

**Presentation.** Desktop automation may be unable to observe the window, for example without a logged-in GUI session on CI. In that case the presentation checkpoint stays `PENDING` with that reason. It is never inferred from a correct readback.

**Hosts.** At least one actual Apple Silicon host must run the GPU and presentation checkpoints, with its chip and macOS version recorded. A host that was not tested is unknown, not passing.

## Negative controls

Each control must end in its own classification, and none may count as a launch pass. They are listed under `negative_controls` in `expected.json`.

| Control | Change | Required outcome |
| --- | --- | --- |
| No Metal | No usable Metal device or capability | Capability unavailable, reported separately from an implementation failure; no frame |
| No frame | The guest loops without submitting a frame | Bounded stop at the worker deadline (#318), classified as no frame |
| Unsupported instruction | A refused instruction before the first frame | Illegal-instruction exit at that instruction; no frame |
| Unresolved import | An import that no service provides | Refused before guest entry |
| Bad shader or binding | An unsupported shader form or an empty binding slot | Rejected at creation or binding; the draw never reaches Metal |
| Wrong expected pixel | One channel of one expected pixel changed in a copy of `expected.json` | Mismatch reported with session, frame, x, y and channel. `--self-test` already checks the static part of this. |
| Low step budget | A budget that ends before the guest exits | Step limit exceeded, not a clean exit |
| Worker timeout | A wall deadline shorter than the run | Timeout, with the last completed stage |

## Records

- A failure in any frame of any session fails the gate, even if a later session or restart succeeds. The record keeps both.
- A truncated, partial or malformed record is incomplete evidence, never a pass.
- GPU checks run with Metal API and shader validation enabled. Any timing is reported separately under the [benchmark protocol](benchmarks.md) and is not part of this gate.

## Reviews

The implementers of #313–#320 do not certify this gate.

- **R1** audits native-arm64 tooling, provenance and the wording of any claim.
- **R2** re-derives the expected values from the fixture source, the rules above and the public specifications, without reading production output, and records the outcome in the [independent review log](review-log.md).

The status above changes from proposed only after both reviews.

## What this does not show

The gate covers one single-module, single-thread original fixture.

- Multi-module startup remains [#256](https://github.com/MacosPorters/betterxen/issues/256).
- Input-driven state remains [#319](https://github.com/MacosPorters/betterxen/issues/319).
- Audio, saves, worker DLL lifecycle, AVX2, wider RDNA 2 coverage and arbitrary file execution are not prerequisites, and passing this gate says nothing about them.
- Nothing here is evidence about commercial titles or Xbox hardware.
