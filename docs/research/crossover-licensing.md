# CrossOver licensing and redistribution boundaries

Research note for the hybrid direction. It records what the public license
texts say about the components a CrossOver-based BetterXen would rely on, and
which documents are still missing. It is not legal advice.

Reviewed 2026-11-25 against public sources only. No CrossOver installation, no
Apple license text and no proprietary component was downloaded for this note.

## Distribution model

BetterXen does not bundle, redistribute or modify CrossOver, D3DMetal, Rosetta 2
or any of the open-source components listed below. The intended model is an
externally installed CrossOver that BetterXen invokes as a separate program, with
all third-party components supplied by their own distributors. This note records
licenses so that any later decision to ship, link or copy a component has a
starting point; it does not approve one.

## Components

| Component | Version or revision read | License read | Source | Redistribution in BetterXen | Notes |
| --- | --- | --- | --- | --- | --- |
| BetterXen | current `main` | GPL-3.0-or-later | [`LICENSE`](../../LICENSE) | The project itself | Compatibility baseline for every row below |
| CrossOver (product) | 26.3.0, per CodeWeavers' source page | Proprietary EULA; FOSS components keep their own licenses | [EULA](https://www.codeweavers.com/crossover/eula), [FOSS source page](https://www.codeweavers.com/crossover/source) | Not bundled; externally installed | EULA §2 permits building alternate Wine for use with CrossOver and states that the EULA does not supersede the FOSS licenses; §3 forbids decompiling or reverse engineering and distribution or derivative works except where law or those licenses permit. The source page publishes the 26.3.0 tarball and lists Wine, Vkd3d, DXVK and MoltenVK; it does not list DXMT or VKD3D-Proton. Whether 26.3.0 ships them, and at which revisions, is **unknown** |
| Wine | `a79470449d03`, master on 2026-11-25 | LGPL-2.1-or-later | [`LICENSE`](https://raw.githubusercontent.com/wine-mirror/wine/a79470449d03/LICENSE) | Not bundled or linked today | "either version 2.1 of the License, or (at your option) any later version" |
| DXMT | `faa73e4a0686`, main on 2026-11-25 | LGPL-2.1-or-later; releases up to v0.80 were MIT | [`LICENSE`](https://raw.githubusercontent.com/3Shain/dxmt/faa73e4a0686/LICENSE), [`LICENSE.OLD`](https://raw.githubusercontent.com/3Shain/dxmt/faa73e4a0686/LICENSE.OLD) | Not bundled or linked today | Copyright 2023–2026 Feifan He for CodeWeavers. `LICENSE.OLD` records MIT for releases up to v0.80, so the licence of a shipped DXMT depends on its version |
| DXVK | `57b2a7c1e87b`, master on 2026-11-25 | zlib | [`LICENSE`](https://raw.githubusercontent.com/doitsujin/dxvk/57b2a7c1e87b/LICENSE) | Not bundled or linked today | Permission to use, alter and redistribute freely; the notice must be retained |
| VKD3D-Proton | `71cae091b065`, master on 2026-11-25 | LGPL-2.1-or-later | [`COPYING`](https://raw.githubusercontent.com/HansKristian-Work/vkd3d-proton/71cae091b065/COPYING) | Not bundled or linked today | `COPYING` grants "either version 2.1 … or any later version"; the `LICENSE` file is the LGPL 2.1 text |
| MoltenVK | `5189f8de86ec`, main on 2026-11-25 | Apache-2.0 | [`LICENSE`](https://raw.githubusercontent.com/KhronosGroup/MoltenVK/5189f8de86ec/LICENSE) | Not bundled or linked today | Compatible with GPL-3.0, not GPL-2.0. There is no `NOTICE` file at this pin, so Apache-2.0 §4(d) does not apply; §4(a)–(c) (license copy, change notices, retained notices) do |
| Apple Game Porting Toolkit repository | `main`, read 2026-11-25 | Apache-2.0 | [`LICENSE`](https://raw.githubusercontent.com/apple/game-porting-toolkit/main/LICENSE) | Not bundled | This is the repository's license for its porting resources. It is not the license for the separately distributed D3DMetal framework |
| D3DMetal | Game Porting Toolkit 4.0, reported | Unconfirmed: a third-party copy of the 4.0 `License.pdf` was read outside this repository; Apple's own download was not read | Third-party copy: `Sikarugir-App/Sikarugir` at `5446b7d18072`, `D3DMetal/{1.1,2.0,2.1,3.0,4.0}/License.pdf` | User-supplied only; never bundled | Reported (evidence class D, not confirmed against Apple's copy): a personal licence to install, use and test for developing, testing or evaluating games on Apple hardware, distribution solely for non-commercial purposes, no use on non-Apple hardware, Framework and Redistributables not separable, and no reverse engineering (sections 2A, 2C, 2D). The missing document is Apple's own copy in the Game Porting Toolkit download. Do not treat the repository's Apache-2.0 license as D3DMetal's |
| Rosetta 2 | macOS feature | Unknown: Apple's applicable software license was not read | [Apple Rosetta documentation](https://developer.apple.com/documentation/apple-silicon/about-the-rosetta-translation-environment) | External OS component; not bundled | The documentation describes the translation environment; it is not the license grant |

## Compatibility with GPL-3.0-or-later

LGPL-2.1-or-later, zlib and Apache-2.0 are generally compatible with a
GPL-3.0-or-later work, subject to each license's notice and source-availability
conditions. That compatibility matters only if BetterXen links to or copies the
component. Invoking an externally installed CrossOver as a separate program does
not make its components part of BetterXen's distributed work, and this note does
not decide otherwise.

The CrossOver EULA is recorded above; D3DMetal's own text is not. Neither may be
bundled, linked or copied on the strength of this note alone: the product terms
are restrictive, and D3DMetal's authoritative licence is still unread.

GPL-2.0-only components cannot be combined into this GPL-3.0-or-later workspace.
A comment on this issue identified MacBox and ScarlettMac as GPL-2.0; they are
not part of the component list above and this note does not rely on them.

## Unknowns and the exact missing documents

- **CrossOver 26.3.0's FOSS inventory.** The EULA points to Appendix A for the
  full list, but that list was not included inline on the page read and was not
  located elsewhere. The source page lists Wine, Vkd3d, DXVK and MoltenVK and
  does not list DXMT or VKD3D-Proton. Whether 26.3.0 ships DXMT, FEX and
  VKD3D-Proton, and at which revisions, is unknown.
- **D3DMetal license.** It ships with the Game Porting Toolkit download; that
  download was not made. A third-party copy of the 4.0 `License.pdf` (in
  `Sikarugir-App/Sikarugir` at `5446b7d18072`) was reported to this issue with a
  section-level summary, but it is not Apple's copy and stays evidence class D.
  The `apple/game-porting-toolkit` repository is Apache-2.0 and does not
  establish the framework's terms. Confirm the summary against Apple's own copy
  before relying on it; until then D3DMetal is user-supplied and never bundled.
- **Rosetta 2 terms.** Apple's license text was not read. Rosetta 2 stays an
  external macOS component and is never redistributed by BetterXen.
- **Exact revisions.** The files above were read at these pins on 2026-11-25:
  Wine `a79470449d03`, DXMT `faa73e4a0686`, DXVK `57b2a7c1e87b`,
  VKD3D-Proton `71cae091b065`, MoltenVK `5189f8de86ec`. The revisions CodeWeavers
  ships in 26.3.0 are still unknown; pin them before any reuse or notice work.
- **Linking versus invocation.** Which components, if any, BetterXen would link
  against rather than invoke is an architecture decision that has not been made
  here.

## Sources

- CodeWeavers, "Free and Open Source Software Code for CrossOver", listing the
  26.3.0 source tarball and the open-source projects:
  <https://www.codeweavers.com/crossover/source> (accessed 2026-11-25).
- Wine `LICENSE`, wine-mirror/wine at `a79470449d03`, accessed 2026-11-25.
- DXMT `LICENSE` and `LICENSE.OLD`, 3Shain/dxmt at `faa73e4a0686`, accessed
  2026-11-25.
- DXVK `LICENSE`, doitsujin/dxvk at `57b2a7c1e87b`, accessed 2026-11-25.
- VKD3D-Proton `COPYING`, HansKristian-Work/vkd3d-proton at `71cae091b065`,
  accessed 2026-11-25.
- MoltenVK `LICENSE`, KhronosGroup/MoltenVK at `5189f8de86ec`, and its file list
  at that pin for the absence of `NOTICE`, accessed 2026-11-25.
- CodeWeavers, "CrossOver End User License Agreement", sections 2 and 3:
  <https://www.codeweavers.com/crossover/eula>, read in a browser 2026-11-25.
- D3DMetal third-party copies: `Sikarugir-App/Sikarugir` at `5446b7d18072`,
  `D3DMetal/{1.1,2.0,2.1,3.0,4.0}/License.pdf`. Not Apple's copy.
- Apple Game Porting Toolkit repository `LICENSE`, apple/game-porting-toolkit at
  `main`, accessed 2026-11-25. Repository license only; it is not the D3DMetal
  license.
- Apple, "About the Rosetta translation environment", accessed 2026-11-25.
  Documentation only, not the license text.
