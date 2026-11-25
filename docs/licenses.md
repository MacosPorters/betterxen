# Dependency license decisions

The workspace is GPL-3.0-or-later. The following is the lead's preliminary compatibility inventory. The independent license audit is pending.

| Component | Version and upstream revision | License used | Scope |
|---|---|---|---|
| objc2 | 0.6.5, `d7d2fa23ceaa5e6096c923b081040e5d81b3b9df` | MIT | Runtime bindings |
| objc2-encode | 4.1.0, `8d214f5477365ffcbcbb7de058c86ed9a518efb7` | MIT | Runtime transitive dependency |
| objc2-foundation | 0.3.2, `7b1abfd750a2cacaea71d6a56ecfb83cb7de560b` | MIT | Foundation bindings |
| objc2-core-foundation | 0.3.2, `7b1abfd750a2cacaea71d6a56ecfb83cb7de560b` | MIT from Zlib OR Apache-2.0 OR MIT | Core Foundation bindings |
| objc2-metal | 0.3.2, `7b1abfd750a2cacaea71d6a56ecfb83cb7de560b` | MIT from Zlib OR Apache-2.0 OR MIT | Metal bindings |
| bitflags | 2.13.2, `80ce9b545acb0bd42150695fc351889cac1d8eb4` | MIT from MIT OR Apache-2.0 | Runtime transitive dependency |
| AMD RDNA 2 ISA XML | 2026-02-20, schema 1.1.1 | MIT | Source for generated decoder constants |
| LLVM AMDGPU tools | 23.1.6 | Apache-2.0 WITH LLVM-exception | External fixture tool, not linked |
| victory-shader (vendored) | victory-mac `22b3f8e45a59`, directory snapshot | GPL-3.0 (same as BetterXen's GPL-3.0-or-later); Mesa-derived opcode table MIT | GFX10-to-DXIL shader recompiler; see its UPSTREAM.md |
| thiserror, zlib-rs, windows, windows-sys | 2.0.21, 0.6.8, 0.62.2, 0.61.2 | MIT OR Apache-2.0; Zlib; MIT OR Apache-2.0 | Dependencies of victory-shader; licences from each crate's metadata, not independently audited |
| DirectX Shader Compiler | v1.9.2609, `dxc_2026_09_29.zip` (sha256 `ad31b1fc…a7e7f1`) | University of Illinois/NCSA plus MIT components (per its licence files) | External tool for compiling HLSL probes under CrossOver; not vendored, not distributed |
| Xcode and Metal tools | Xcode 27.2 | Apple proprietary terms | External host tools, not distributed |

## Runtime redistribution review

Reviewed 2026-11-22 against the locked Apple Silicon graph. Run `cargo tree --locked --target aarch64-apple-darwin --edges normal,build` and `cargo metadata --locked --format-version 1` to reproduce the graph and declared licenses. The external runtime graph comprises the six Cargo packages listed above. The reviewed registry packages identify their source revisions in `.cargo_vcs_info.json`. The five objc2 registry packages declare the listed licenses but contain no standalone license text. Each of the three pinned upstream `LICENSE.md` files matches the preserved copy byte for byte.

The declared MIT terms are compatible with GPL-3.0-or-later for code the upstream authors have rights to license. MIT remains the selected option for objc2-core-foundation, objc2-metal and bitflags. Distribution of copies or substantial portions subject to MIT requires retaining the applicable copyright and permission notice. Preserve the project GPL text, AMD's MIT notice, the bitflags MIT notice, and the objc2 notices. For a linked Xenora binary, also satisfy the GPLv3 section 6 Corresponding Source requirements for the shipped build, including applicable linked dependency source and build scripts, subject to the license's System Libraries exception. See the [project GPL text](../LICENSE).

The supplied objc2 MIT text comes from upstream commit `ee9a7ada2131f5944b8750428e265c15632f2a19`. Upstream confirms that the older crate versions use the license declared in their manifests, but did not confirm that the added copyright notice covers all earlier contributions. The text is preserved as upstream supplied it. Attribution completeness for the locked releases still requires review of applicable earlier notices. [Upstream MIT text](https://github.com/madsmtm/objc2/blob/ee9a7ada2131f5944b8750428e265c15632f2a19/LICENSE-MIT.txt) · [Maintainer discussion](https://github.com/madsmtm/objc2/issues/826)

### What the maintainer resolved

In issue #895, the maintainer confirmed that the published terms cover
generated bindings to the extent upstream can license them and that
upstream imposes no additional downstream obligations. Normal license
notice conditions still apply. In the reply specific to Xenora, the
maintainer explained that generated Foundation bindings are inline
wrappers around calls into the Apple framework. The response resolves
upstream's licensing position and explains where the Apple
implementation executes. The maintainer identified no Apple guidance
for this case and left Apple-specific rights to the applicable terms.
[Issue #895](https://github.com/madsmtm/objc2/issues/895)
· [Xenora-specific reply](https://github.com/madsmtm/objc2/issues/895#issuecomment-7079277040)

### Pinned attribution findings

The 2026-11-23 audit matched all six runtime package versions, source
revisions and declared licenses to the locked native build graph and
each crate's registry metadata. The three pinned objc2 root licensing
notices and the later supplied MIT text match the preserved copies
byte for byte. The bitflags MIT notice matches its packaged file. A
search of the shipped Rust source for copyright and SPDX markers found
no additional notices. Keep both supplied objc2 texts. Issue #826
confirms the older manifest declarations, while the completeness of
the later copyright line for earlier contributions remains unconfirmed.
The audit records the available notices for the exact pins and leaves
that specific release attribution question open.
[Maintainer attribution discussion](https://github.com/madsmtm/objc2/issues/826)

### Content of the native release build

Built the locked CLI with
`cargo build --locked --release --target aarch64-apple-darwin -p xenora`
at `4b9a4cecf35c8d4d90d8ecdb9c1510e7bd260102`, with CLI and dependency
inputs unchanged. The inspected artifact was the standalone ARM64
Mach-O executable, SHA-256
`341c4d0c2835c82a1b199b2c9079b8ee3113657b2ca418f0214c569bb2a1b184`.
It imports Metal, Foundation and CoreFoundation from macOS system
framework paths, plus libobjc and libSystem. No Apple frameworks
were bundled with the inspected executable.

The executable retains more than a list of dynamic imports.
`nm` and disassembly show generated `NSError::localizedDescription`
and `MTLCompileOptions::setMathMode` wrapper code, which dispatches via
`objc_msgSend` and includes selector caching or return-value handling.
It also contains generated binding symbol names and selector strings,
and release location strings name generated Rust source files. No
embedded DWARF section was found. These observations distinguish retained
Rust glue and metadata from the Apple framework implementations called
at runtime. They do not establish the provenance of every byte or
the result for another link or packaging configuration. Reproduce with
`file`, `lipo -archs`, `otool -L`, `otool -l`, `otool -tvV`,
`nm -m` and `strings -a`. Raw records are under `artifacts/g11/`.

### Remaining release question

G11 remains open for redistribution. Xcode agreement §2.4 addresses
distribution of qualifying macOS applications and libraries. §2.7
restricts redistribution and derivative works of Apple Software
subject to stated exceptions. The remaining question is whether the
applicable terms cover any Apple-derived expression actually conveyed
in retained wrappers or metadata, and the SDK-derived Rust declarations
conveyed in any matching or vendored source bundle. Confirm the terms
for the actual distribution, its pinned attribution and GPLv3 §6
Corresponding Source scope. The local binary inspection does not
qualify a future release archive, symbols package or source offer.
This release question leaves local clean-room research and Phase 02
implementation under their existing engineering gates.
[Apple Xcode and SDKs Agreement](https://www.apple.com/legal/sla/docs/xcode.pdf)

The lockfile also lists optional dispatch2 0.3.1 (revision
`8852b424193ca41602281b3d7540d7c8ed51e49a`, MIT selected). It is absent
from the native normal/build graph, including with all workspace
features enabled.

## Fuzz development dependencies

These packages are resolved only by the isolated fuzz workspace. Use MIT when it is an offered choice. The bundled LLVM libFuzzer source in `libfuzzer-sys` also carries NCSA terms. Fuzz executables are local test artifacts.

| Component | Locked version | Upstream revision | Selected license |
|---|---|---|---|
| arbitrary | 1.5.0 | `9807b599146c301a67e20692635352c9db70bff2` | MIT |
| cc | 1.11.3 | `c548c664cb6d71692959c1bd98959adc52ef7da7` | MIT |
| cfg-if | 1.0.5 | `8b0a2b0fc1fa3b3741b9e72895f390cf499fe36f` | MIT |
| find-msvc-tools | 0.1.15 | `ffbb10da608f715ed69a4561c4cbdca7c6fbbba6` | MIT |
| getrandom | 0.4.3 | `5e7cd5733536844a9856dc7259bd4696bbe5e3ae` | MIT |
| jobserver | 0.1.35 | `85ba04e3cf89e5c10e7b82de69f6be6dff9d3ec3` | MIT |
| libc | 0.2.191 | `ccc2ff4aaab962f7fde7b8edda7379c045b1ca12` | MIT |
| libfuzzer-sys | 0.4.13 | `719e4efb9b8857ebaa782ae59376c8cbb78fed0f` | MIT AND NCSA |
| r-efi | 6.0.0 | `7e1b0322d31d625f81a5656096330934f9cd835d` | MIT |
| shlex | 2.0.1 | `e82b1411beb7c92871c2c078c9ab415bbcf207ef` | MIT |

## External hybrid components (not distributed)

The hybrid direction relies on an externally installed CrossOver. Nothing below is bundled, linked or redistributed by BetterXen, and the licenses are the upstream texts read on 2026-11-25, not the revisions CodeWeavers ships in 26.3.0.

| Component | License read | Distribution model | Notes |
|---|---|---|---|
| CrossOver (product) | Proprietary EULA; FOSS components keep their licenses | External install | [EULA](https://www.codeweavers.com/crossover/eula) §2 permits building alternate Wine and does not supersede FOSS licenses; §3 restricts reverse engineering and redistribution. The source page lists Wine, Vkd3d, DXVK and MoltenVK, not DXMT or VKD3D-Proton |
| Wine | LGPL-2.1-or-later | External | `either version 2.1 … or any later version` |
| DXMT | LGPL-2.1-or-later; releases up to v0.80 were MIT (`LICENSE.OLD`) | External | Copyright 2023-2026 Feifan He for CodeWeavers |
| DXVK | zlib | External | Retain the notice if ever distributed |
| VKD3D-Proton | LGPL-2.1-or-later | External | Source headers carry the `or later` grant |
| MoltenVK | Apache-2.0 | External | Compatible with GPL-3.0, not GPL-2.0; no `NOTICE` file at the pinned revision |
| D3DMetal | Unverified: a third-party copy of the 4.0 `License.pdf` was reported; Apple's own copy was not read | User-supplied only; never bundled | The reported terms are non-commercial and no reverse engineering; the Apache-2.0 Game Porting Toolkit repository is not the D3DMetal license |
| Rosetta 2 | Unknown; Apple's license text was not read | External OS component | Never redistributed |

Details, exact sources and the missing documents are in [research/crossover-licensing.md](research/crossover-licensing.md).

