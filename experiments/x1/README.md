# Synthetic guest execution experiments

**Preserved native research.** This fixture or validation procedure is for the paused independent native path, not CrossOver integration. Retain existing regression checks; proposed expansion is historical unless a specific hybrid need or maintainer decision authorizes it. See the [active architecture](../../docs/architecture.md).

The bounded x86-64 guest interpreter (X1) runs on native ARM64 macOS.

This isolated Cargo workspace implements the original fixtures in the
[Phase 02 feasibility study](../../docs/research/phase02-cpu-feasibility.md).
Both flat fixtures use immutable code at guest address `0x1000` and an
eight-byte writable output at `0x2000`. The interpreter takes the end of the
code slice as the terminal address and validates each branch target from offset
zero before executing. The program prints initial state, each retired state and
the terminal state with output. The tests use literal expected RIP, RAX,
arithmetic flags and output bytes.

The checked-in fixture bytes are staged separately from the Rust decoder.
`fixtures/check.sh` recreates the bytes from the assembly and prints tool
versions, architecture and linkage, fixture bytes and SHA-256 hashes. The
assembler emits a temporary x86-64 relocatable object and the script extracts
its text. The host runs the LLVM tools and Rust interpreter as ARM64 processes.

## Supported instruction subset

The interpreter decodes and executes the following x86-64 forms. All other
encodings trap with `Unsupported`. Partial encodings (truncated immediates or
displacements) trap with `Truncated`.

| Encoding | Instruction | Notes |
| --- | --- | --- |
| `B8 id` | `MOV EAX, imm32` | Zero-extends to RAX. |
| `83 C0 01` | `ADD EAX, 1` | Full ARITHMETIC flags (CF PF AF ZF SF OF). |
| `83 E8 01` | `SUB EAX, 1` | Full ARITHMETIC flags including borrow (CF) and AF. |
| `83 F8 ib` | `CMP EAX, imm8` | Sets flags like SUB; EAX unchanged. |
| `74 cb` | `JE rel8` | Jumps if ZF=1. Target validated from offset 0. |
| `75 cb` | `JNE rel8` | Jumps if ZF=0. Target validated from offset 0. |
| `48 89 07` | `MOV [RDI], RAX` | Writes 8 bytes; faults if outside output range. |
| `0F 0B` | `UD2` (service trap) | Dispatches by EAX through the active service handler. |

Branch targets are validated by scanning the code from offset zero to confirm
the target address lies on an instruction boundary. Both forward and backward
branches are permitted; the step budget (`MAX_STEPS=32`) prevents infinite
loops.

The initial register state used by the flat-fixture tests sets RDI to the
8-byte output base. Only EAX and RFLAGS change during arithmetic operations;
other registers are preserved by all non-service instructions.

Run from the repository root after the active fuzz campaign completes.

```sh
set -e
set -o pipefail
manifest=experiments/x1/Cargo.toml
export CARGO_TARGET_DIR="$PWD/artifacts/x1/target"
export CARGO_BUILD_JOBS=2
mkdir -p artifacts/x1

LLVM_BIN="$(brew --prefix llvm)/bin" bash experiments/x1/fixtures/check.sh \
  2>&1 | tee artifacts/x1/fixtures.log
cargo fmt --manifest-path "$manifest" -- --check
cargo clippy --manifest-path "$manifest" --locked --target aarch64-apple-darwin \
  --all-targets -- -D warnings
cargo test --manifest-path "$manifest" --locked --target aarch64-apple-darwin \
  2>&1 | tee artifacts/x1/tests.log
cargo run --manifest-path "$manifest" --locked --target aarch64-apple-darwin \
  2>&1 | tee artifacts/x1/trace.log
driver="$CARGO_TARGET_DIR/aarch64-apple-darwin/debug/xenora-x1"
{ uname -m && sw_vers && file "$driver" && lipo -archs "$driver" && otool -L "$driver"; } \
  2>&1 | tee artifacts/x1/native.log
```

## Fixed-base 64-bit Portable Executable loading (X2)

X2 interprets an original synthetic AMD64 guest image. The loader uses the
header, directory and section layouts from the [Microsoft PE format specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).
It accepts a narrow profile with a fixed image base of `0x140000000`, a
512-byte header page and two disjoint 512-byte raw section pages. Guest
addresses translate to the declared virtual bytes of immutable RX code at
`0x140001000` and eight writable data bytes at `0x140002000`. Raw alignment
padding is outside both guest ranges. The entry must be the start of code.
The image must declare stripped relocations and all 16 data directories
must be empty. Arbitrary Windows executables and game execution are outside
this experiment's scope.

The initial state sets RCX to 41, RDI to the data address, RFLAGS to 2 and
all other registers to zero. The guest loads service ID 1 into EAX. In X2,
`UD2` dispatches the synthetic service selected by RAX. Service 1 adds one
to RCX modulo 2⁶⁴ and writes the result to RAX, preserving RCX and RFLAGS.
It resumes two bytes after the trap. An unknown ID faults at the trap RIP
before retirement. The final instruction stores RAX to the data range.
Execution ends at the first byte past the declared code range.

| Event | RIP after event | RAX | RCX |
| --- | --- | --- | --- |
| Entry | `0x140001000` | 0 | 41 |
| Load service ID | `0x140001005` | 1 | 41 |
| Return from trap | `0x140001007` | 42 | 41 |
| Store and finish | `0x14000100a` | 42 | 41 |

Run from the repository root.

```sh
python3 experiments/x1/fixtures/x2.py --check
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/artifacts/x2/target"
cargo test --manifest-path experiments/x1/Cargo.toml --locked --target aarch64-apple-darwin
cargo run --manifest-path experiments/x1/Cargo.toml --locked --target aarch64-apple-darwin -- x2
```

## Guest-to-GPU command submission (X3)

X3 uses an original AMD64 PE32+ fixture with fixed image base `0x400000`.
The shared X2 loader validates the PE entry and maps the declared virtual
bytes of RX code at `0x401000`, eight writable status bytes at `0x402000`,
and 48 read-only bytes at `0x403000`. The lower image base keeps the command
address representable by the interpreter's existing `MOV EAX, imm32`.
The read-only section holds the 32-byte command followed by the 16-byte
input backing handle 1. The PE initializes status to eight `0xa5` bytes.
The host initializes RDI to the status address. The X1 interpreter starts
at the validated PE entry. Guest code loads the command address into EAX,
traps with `UD2`, then stores RAX through RDI.

The command contains eight little-endian 32-bit fields in order.

| Field | Fixture value | Validation |
| --- | --- | --- |
| Version | 1 | Must be 1 |
| Service ID | 2 | Must be 2 |
| Kernel ID | 1 | Selects the original trusted OR kernel |
| Input handle | 1 | Selects the fixture's 16-byte input |
| Access mode | 1 | Read-only |
| Input byte offset | 4 | Word aligned and within the selected input |
| Input byte length | 12 | Must equal four times the grid word count |
| Grid word count | 3 | Range 1 through 4 |

The trap checks the full command address and input range before it hands
owned input words to Metal. Each invocation ORs one input word with
`0x00ff00ff`. `NativeCompute::run_words` waits for command completion and
returns owned readback. Success returns zero in RAX for the guest to store.
Malformed commands and backend failures fault at the trap without retiring
it. The guest status retains its initial bytes in those cases. The host
compares the completed readback with three literal expected words. This
protocol is local to X3 and covers synthetic compute only.

The default build has no Metal dependency. Run from the repository root
with an external wall-clock limit around the Metal invocation.

```sh
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/artifacts/x3/target"
manifest=experiments/x1/Cargo.toml
target=aarch64-apple-darwin
python3 experiments/x1/fixtures/x3.py --check
cargo test --manifest-path "$manifest" --locked --target "$target" -- --nocapture
cargo build --manifest-path "$manifest" --locked --target "$target" --features metal
env MTL_DEBUG_LAYER=1 MTL_SHADER_VALIDATION=1 MTL_SHADER_VALIDATION_REPORT_TO_STDERR=1 \
  MTL_SHADER_VALIDATION_ABORT_ON_FAULT=1 \
  python3 -c 'import subprocess, sys; sys.exit(subprocess.run(sys.argv[1:], timeout=30).returncode)' \
  "$CARGO_TARGET_DIR/$target/debug/xenora-x1" x3
```

## Guest-driven Metal rendering (X4)

Two original PE fixtures use the X3 image layout and the X1 trap path.
Each carries a 32-byte command followed by 16 read-only parameter bytes.
The little-endian command fields are version (1), service (3), target handle
(1), width (8), height (8), format (1 for RGBA8Unorm), parameter offset,
and parameter length (4). The offset is relative to the parameter
area and must select an aligned, in-bounds 32-bit value in 0–255.
Handle 1 identifies the sole 8×8 target for that invocation. The guest
status stays pending if validation or Metal submission fails.

The experiment uses the existing native offscreen test's Metal command
and texture pattern in its own graphics module. A full-screen triangle
writes a single shared texture. After Metal completion, the runner reads
level zero and compares all 256 bytes with a row-major CPU expectation
of `[x, y, p, 255]`. The two fixtures supply `p=17` and `p=93`.
The run prints capabilities, completion, guest status, and readback bytes.
This covers a synthetic fixture only. Game execution is unsupported.

Run from the repository root.

```sh
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/artifacts/x4/target"
export PYTHONDONTWRITEBYTECODE=1
manifest=experiments/x1/Cargo.toml
target=aarch64-apple-darwin
python3 experiments/x1/fixtures/x4.py --check
cargo test --manifest-path "$manifest" --locked --target "$target" --features metal
cargo build --manifest-path "$manifest" --locked --target "$target" --features metal
env MTL_DEBUG_LAYER=1 MTL_SHADER_VALIDATION=1 MTL_SHADER_VALIDATION_REPORT_TO_STDERR=1 \
  MTL_SHADER_VALIDATION_ABORT_ON_FAULT=1 \
  python3 -c 'import subprocess, sys; sys.exit(subprocess.run(sys.argv[1:], timeout=30).returncode)' \
  "$CARGO_TARGET_DIR/$target/debug/xenora-x1" x4
```

## Multi-service countdown dispatch (X5)

X5 is an original synthetic PE32+ fixture that exercises typed service dispatch
through a typed `ServiceRegistry`, a backward-branch countdown loop, and a
data-section store — all without any `#[cfg(test)]` gate. The image uses the
same two-section X2 profile: `.text` RX at `0x140001000`, `.data` RW at
`0x140002000`; image base `0x140000000`.

### Code layout (30 bytes)

| Offset | Bytes | Instruction | Effect |
| --- | --- | --- | --- |
| 0–4 | `b8 01 00 00 00` | `MOV EAX,1` | Load service-1 ID |
| 5–6 | `0f 0b` | `UD2` | Service 1 (identity): EAX ← RCX |
| 7–9 | `83 f8 00` | `CMP EAX,0` | Is N zero? |
| 10–11 | `74 06` | `JE +6` → [18] | Skip loop body if N=0 |
| 12–14 | `83 e8 01` | `SUB EAX,1` | N-- |
| 15–17 | `83 f8 00` | `CMP EAX,0` | Check decremented value |
| 18–19 | `75 f3` | `JNE -13` → [7] | Loop back to CMP if N≠0 |
| 20–24 | `b8 02 00 00 00` | `MOV EAX,2` | Load service-2 ID |
| 25–26 | `0f 0b` | `UD2` | Service 2 (double): EAX ← RCX×2 |
| 27–29 | `48 89 07` | `STORE [RDI]` | Write result to guest `.data` |

### Services

| ID | Name | Returns |
| --- | --- | --- |
| 1 | identity | `RCX` (the initial argument N) |
| 2 | double | `RCX × 2` (wrapping) |

### Independent expected outputs

| N (RCX) | Loop iterations | `.data` bytes |
| --- | --- | --- |
| 0 | 0 (JE taken) | `[00, 00, 00, 00, 00, 00, 00, 00]` |
| 1 | 0 (JNE falls) | `[02, 00, 00, 00, 00, 00, 00, 00]` |
| 2 | 1 | `[04, 00, 00, 00, 00, 00, 00, 00]` |
| 3 | 2 | `[06, 00, 00, 00, 00, 00, 00, 00]` |

Expected values are derived from the program listing before running the code.

Run from the repository root.

```sh
python3 experiments/x1/fixtures/x5.py --check
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/artifacts/x5/target"
cargo test --manifest-path experiments/x1/Cargo.toml --locked --target aarch64-apple-darwin
cargo run --manifest-path experiments/x1/Cargo.toml --locked --target aarch64-apple-darwin -- x5
```


## Multiple guest functions and mapped memory (X6)

X6 runs an original fixed-base PE32+ guest through its entry point,
countdown routine, and result-store routine. Each case gets fresh guest
memory and stack state. The cases write 0, 2, 4, and 6 and retire 12, 16,
20, and 24 instructions, respectively.

Mappings enforce read, write, and execute permissions and reject cross-region
accesses. Taken branches decode from their destination byte. `RET` verifies
the expected stack slot and return value. A root `RET` exits after consuming
the root slot. This loader profile accepts directory-free images.

```sh
python3 experiments/x1/fixtures/x6.py --check
cargo run --locked --manifest-path experiments/x1/Cargo.toml --target aarch64-apple-darwin -- x6
```

## Named synthetic imports (X7)

X7 supports a fixed-base test PE32+ image with named imports from
`XENORA-TEST.dll`. The loader checks the import directories and resolves
each named slot before entry. Calls through bound slots check the guest
stack frame and dispatch synthetic services. The services validate the
entire guest buffer before reading or writing it.

Build the separate [compatibility fixtures](../../tests/compat_fixtures/README.md)
and pass the resulting executable to the interpreter. The positive fixture
prints `[4, 10, 22]` for both services and guest exit status `36`. The CLI
returns zero when interpretation completes. The negative fixture returns a
host failure and reports the unresolved symbol before guest entry.

```sh
tests/compat_fixtures/build.sh
cargo run --locked --manifest-path experiments/x1/Cargo.toml --target aarch64-apple-darwin -- x7 artifacts/compat-fixtures/guest-0.exe
cargo run --locked --manifest-path experiments/x1/Cargo.toml --target aarch64-apple-darwin -- x7 artifacts/compat-fixtures/guest-1.exe
```

X7 uses one interpreter thread and two synthetic services. Relocations,
thread-local storage, and general Windows or Xbox executable loading remain
unsupported.

## Guest compute readback (X8)

Run `tests/compat_fixtures/verify-compute.sh` on native ARM64 macOS with LLVM
and Metal. The independently compiled PE32+ guest calls the synthetic
`xenora_compute` import through the named-import runner. The guest initializes
three words, submits a checked descriptor, reports the GPU output and
returns its sum. The gate repeats the run and checks rejection of an unknown
command, invalid handle, invalid binding, wrong count, unmapped buffer and
reused one-shot handle.

## Persistent guest resources and translated offscreen rendering (X9)

The Metal-feature runner accepts `x9 <synthetic-pe-path>`. This loader
profile resolves `xenora_resource` and `xenora_report` from
`XENORA-TEST.dll`. It uses the same fixed-base named import resolution as
X7 and retires at most 512 guest instructions.

The resource import takes a four-byte-aligned guest request address in RCX
and a guest data address or zero in RDX. A request is five little-endian
32-bit words in the order opcode, handle, offset, length and kind. Unused
fields are zero.

| Opcode | Request fields | RDX | Return value |
| --- | --- | --- | --- |
| 1 | Length in bytes and kind 1 (32-bit words) or 2 (vertex vectors) | Readable source | Handle |
| 2 | Handle, byte offset and byte length | Readable replacement bytes | Bytes updated |
| 3 | Handle | Zero | Zero |
| 4 | Handle, byte offset and byte length | Zero | Queued XOR input bytes |
| 5 | Expected output length in bytes | Writable destination | Bytes written |
| 6 | Handle, byte offset, length 96 and program 1 (constant) or 2 (varying) | Zero | Queued vertex bytes |
| 7 | Block payload length in bytes | Readable texture descriptor followed by the block payload | Handle |
| 8 | Handle, array layer in offset and mip level in length | Zero | Queued texture output bytes |

The runtime owns eight slots of up to 4096 resource bytes each. A handle
carries a process-wide generation and a slot. Releasing the slot invalidates
that handle even if a subsequent allocation reuses the slot. Buffer creation
copies guest data, and updates require aligned spans within the resource.
Texture resources are immutable after creation. Their descriptor is eight
little-endian 32-bit words in the order width, height, mip count, layer
count, format (1 for BC1), usage (1 for sampled), row pitch and slice pitch.
The payload stores consecutive mip chains in array-layer order. Pitches are
in bytes and identical across mips.

One queued operation retains an immutable resource snapshot. Updating or
releasing a handle preserves the queued bytes. The finish operation validates
the complete destination and runs the queued operation synchronously on
Metal. The draw viewport is 4×3. Its input is three little-endian
floating-point position vectors followed by three color vectors, each with
four components. Program selection identifies one of the original gfx1030
vertex and pixel fixture pairs. The shader compiler translates both stages
to Metal Shading Language. The draw and texture readback produce tightly
packed red, green, blue and alpha bytes in row order. Texture readback
uses an authored Metal kernel with an output limit of 4096 texels. The
runner retains at most 16 completed readbacks for fixture output.

Guest exit drops resources and any queued operation. Queued work is still a
CPU snapshot and creates native resources when finished. Asynchronous GPU
submission, release of in-flight Metal allocations, guest-supplied shader
code, and translated image instructions are outside this profile. The
[guest resource gate](../../tests/compat_fixtures/README.md) documents
the original fixture and checks.
