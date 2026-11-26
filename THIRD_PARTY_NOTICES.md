# Third-party notices

BetterXen's own source is `GPL-3.0-or-later`. The active launcher uses `libc` 0.2.191 (`ccc2ff4aaab962f7fde7b8edda7379c045b1ca12`) for POSIX signals and file locks, under its MIT option. Preserve the Rust Project Developers notice and permission text. Private build bundles include the unmodified locked libc source and notices.

The memory guard and stall/memory-settle concepts in `launcher/crates/bx-runtime` are inherited from BetterXen #405: attributed Rust reimplementations of contributor-owned supervision concepts from skakwy/victory-mac at `22b3f8e45a59`, GPL-3.0, used with the author's permission as recorded in the [donor per-component audit](https://github.com/MacosPorters/betterxen/blob/b42133add913297690dbade1941916f4ce0b666a/docs/research/upstream-integration-audit.md). No game-facing implementation is copied. The excluded `victory-shader` compiler is not imported.

Original conformance workloads are built with external llvm-mingw 20261117 (archive SHA-256 `98d340bbb8d46b09ad6886ffba88bd9d5415e131c316de48e6bd7647377446aa`). Its wrappers are ISC; LLVM and MinGW compiler/runtime components carry separate terms. Compiler binaries are not bundled. Windows workload bundles preserve the unmodified LLVM/MinGW notices for linked runtime support. Microsoft DXC v1.9.2609 is an external, checksum-pinned Windows guest tool; its executable and DLLs are never bundled. Details: [selected source review](docs/research/maintainer-upstream-review.md).

## External Host Tools & Runtime Dependencies (Never Bundled)

| Component | Role | Licensing & Provenance |
| --- | --- | --- |
| CrossOver / Wine | External macOS Windows compatibility runtime | Proprietary product with FOSS subcomponents; externally installed by the user and never bundled. See [docs/research/crossover-licensing.md](docs/research/crossover-licensing.md). |
| Apple D3DMetal & Rosetta 2 | External Direct3D-to-Metal and x86-64 translation | Proprietary Apple components under Apple's license terms; externally installed and never bundled. |
| `mingw-w64` & CMake | Developer cross-compilation toolchain for x86-64 PE32+ shims | Installed externally via Homebrew (`brew install mingw-w64 cmake`) for building `shims/` and `tests/conformance/`. |

Historical third-party notices for the retired native-first workspace (including AMD RDNA 2 ISA metadata, `bitflags`, and `objc2-*`) remain preserved at Git tag `native-first-final` and are summarized in [docs/licenses.md](docs/licenses.md).
