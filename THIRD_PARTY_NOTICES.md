# Third-party notices

The active BetterXen Rust workspace (`Cargo.toml` / `Cargo.lock`) is original `GPL-3.0-or-later` source with **one external crates.io dependency**: `libc`, used only by `bx-cli`.

`bx-cli` links the unmodified `libc` 0.2.191 crate (`ccc2ff4aaab962f7fde7b8edda7379c045b1ca12`) for POSIX process groups and signals, under its MIT option. The crate is licensed `MIT OR Apache-2.0`; its notice and permission text (`LICENSE-MIT` in the 0.2.191 crate) are reproduced under [libc MIT license text](#libc-mit-license-text) below.

## Vendored source files

| Files | Origin | License |
| --- | --- | --- |
| `shims/xcrdapi/dstorage/dstorage.h`, `shims/xcrdapi/dstorage/dstorageerr.h` | Microsoft DirectStorage SDK headers, NuGet `Microsoft.Direct3D.DirectStorage` 1.4.0, unmodified (#483). Provenance and hashes: [shims/xcrdapi/dstorage/README.md](shims/xcrdapi/dstorage/README.md). | MIT, per-file notice. The package's `LICENSE-CODE.txt` is kept next to the headers and reproduced under [DirectStorage headers MIT license text](#directstorage-headers-mit-license-text) below. |

## External Host Tools & Runtime Dependencies (Never Bundled)

| Component | Role | Licensing & Provenance |
| --- | --- | --- |
| CrossOver / Wine | External macOS Windows compatibility runtime | Proprietary product with FOSS subcomponents; externally installed by the user and never bundled. See [docs/research/crossover-licensing.md](docs/research/crossover-licensing.md). |
| Apple D3DMetal & Rosetta 2 | External Direct3D-to-Metal and x86-64 translation | Proprietary Apple components under Apple's license terms; externally installed and never bundled. |
| `mingw-w64` & CMake | Developer cross-compilation toolchain for x86-64 PE32+ shims | Installed externally via Homebrew (`brew install mingw-w64 cmake`) for building `shims/` and `tests/conformance/`. |

Historical third-party notices for the retired native-first workspace (including AMD RDNA 2 ISA metadata, `bitflags`, and `objc2-*`) remain preserved at Git tag `native-first-final` and are summarized in [docs/licenses.md](docs/licenses.md).

## libc MIT license text

```text
Copyright (c) The Rust Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

## DirectStorage headers MIT license text

```text
Copyright (c) Microsoft Corporation.

MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED *AS IS*, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
