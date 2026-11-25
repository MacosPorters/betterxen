# Registry verification (2026-11-25)

Independent re-derivation of [the upstream research registry](upstream-registry.md)
and [the component reuse matrix](component-reuse-matrix.md), as issue #363 asks.
Each row was checked from primary files and repository metadata, not from the
registry's conclusions. Evidence class D: public files read on 2026-11-25;
nothing installed, built or run. Not legal advice.

## Method

- **Pin:** `GET /repos/{owner}/{repo}/commits/{pin}` — a row fails if the recorded
  short SHA does not resolve to a commit.
- **License:** the repository's own license file read at the recorded revision or
  default branch, not GitHub's detected label alone. Where the two differ, both
  are recorded.
- **Role:** the project's README text, quoted only for what it states about itself.
- **Provenance or runtime behaviour** is never inferred from repository metadata;
  those stay `unknown`.

## Row-by-row result

| Registry row | Pin | License re-derived | Independent finding | Status |
| --- | --- | --- | --- | --- |
| CrossOver (CodeWeavers) | 26.3.0 source listed | Proprietary product; FOSS components listed | The source page offers the 26.3.0 tarball and names Wine, Vkd3d, DXVK, MoltenVK and others; it is not the product licence | Confirmed for the source page; product terms **unknown** |
| Wine | `a79470449d03` | LGPL-2.1-or-later (`LICENSE`, `COPYING.LIB`); GitHub reports NOASSERTION | License text grants "either version 2.1 … or any later version". GitHub's detection is not authoritative | **Confirmed** (the registry's "LGPL 2.1" should say or-later) |
| DXMT | `faa73e4a0686` | LGPL-2.1-or-later (`LICENSE`) | Copyright 2023–2026 Feifan He for CodeWeavers; same or-later grant | **Confirmed** |
| DXVK | `57b2a7c1e87b` | zlib (`LICENSE`) | Permission to use, alter and redistribute freely with notice | **Confirmed** |
| VKD3D-Proton | `71cae091b065` | LGPL-2.1 (`LICENSE`); source headers grant "or later" | README states a Vulkan 1.3 requirement | **Confirmed**; registry should say LGPL-2.1-or-later |
| MoltenVK | `5189f8de86ec` | Apache-2.0 (`LICENSE`) | — | **Confirmed** |
| FEX | `3ee25399232a` | MIT (`LICENSE`) | README: "run x86 applications on ARM64 Linux devices" | **Confirmed** |
| VIctory | `13f91caee25f` | GPL-3.0 (`LICENSE`) | README: runs the Xbox Series X\|S build of a commercial title from user-supplied files | License and role **confirmed**; the "leaked" provenance is third-party only — **unknown** |
| VIctory-macOS | `485b230ef596` | GPL-3.0 (`LICENSE`) | README names Rosetta, CrossOver, D3DMetal, Wine and Metal | **Confirmed** |
| PortWatch VI | `ed00263cc0c9` | No license reported (NOASSERTION) | A website/tracker, not a code dependency | **Confirmed** as a reference only |
| llvm-mingw (additional) | `903f4cf7faa2` | `LICENSE.txt`: ISC for llvm-mingw's own scripts and wrappers; the built toolchain carries its components' licenses | GitHub reports "other", which hid the actual grant | **Corrected**: record ISC for the project files and component licences for the built toolchain |
| Sikarugir (additional) | `5446b7d18072` | No license reported | README: successor to Wineskin, macOS 14.6 or later | **Confirmed**; no reuse |
| macOS Wine builds (additional) | `72f8915f2d01` | No license reported | Community Wine builds; provenance unreviewed | **Confirmed** as unreviewed reference |
| Mythic (additional) | `754223978e61` | GPL-3.0 (`LICENSE.md`) | Launcher for Windows games through Wine | **Confirmed** |
| Playdock (additional) | `f8b8ef7fb70` → **does not resolve** | MIT (`LICENSE`) | README: "A free alternative to CrossOver", built on the Sikarugir engine | **Pin corrected**: the head is `f58b8ef7fb70ffbb2756792266a829c6b090a83b` |

## Corrections to apply to the registry

1. **Playdock pin** `f8b8ef7fb70` is not a commit; the correct short pin is
   `f58b8ef7fb70`.
2. **llvm-mingw licence**: `LICENSE.txt` is the ISC licence for llvm-mingw's own
   scripts, wrappers and test cases; the generated toolchain is covered by the
   licences of the components it bundles. "GitHub: other" is not the fact.
3. **VKD3D-Proton** and **Wine** should be recorded as LGPL-2.1-or-later, which is
   what their files grant; the registry's "LGPL 2.1" is the base version only.

All other pins resolve, all other licence readings match, and the role
descriptions checked above match their READMEs.

## Reuse-matrix decisions re-checked

| Decision | Rests on | Result |
| --- | --- | --- |
| Adopt CrossOver as an external dependency | Product boundary, not a licence fact | Stands as a strategic decision; the product EULA is still unread |
| Wine as a reference | LGPL-2.1-or-later | Stands; copying needs the per-file review the matrix already requires |
| Rosetta 2 permitted where Apple supports it | Apple's support lifetime | Stands only as a condition: Apple's two notices conflict on the final release (#348), so no end date is established |
| FEX not applicable now | README targets ARM64 Linux | Stands |
| D3DMetal external, not redistributable until proven | Licence unknown | Stands; the unknown is unchanged |
| DXMT evaluated as external | LGPL-2.1-or-later | Stands; linking or copying still needs review |
| DXVK only with MoltenVK | zlib plus a Vulkan provider | Stands; macOS value unknown |
| VKD3D-Proton blocked on Vulkan 1.3 | README requirement | Stands; MoltenVK coverage unknown |
| MoltenVK evaluated | Apache-2.0 | Stands |
| VIctory and VIctory-macOS rejected for reuse | Provenance of the input | The rejection stands as a risk decision, but "leaked build" is a third-party tracker claim; public repository metadata does not establish it. Label it accordingly |
| PortWatch VI reference only | Editorial tracker, no licence | Stands |
| Native implementation preserved and paused | Project policy | Not a factual row; nothing to verify |
| Test and diagnostics methodology reused | Project policy | Not a factual row; nothing to verify |

## Remaining unknowns

- CrossOver's product EULA and D3DMetal's licence text (#354).
- Rosetta 2's final supported macOS release: Apple's own notices conflict.
- The provenance of VIctory's inputs. Only a third-party tracker states it.
- Runtime behaviour, compatibility and performance of every row; none was run.
- The exact revisions CodeWeavers ships in 26.3.0 versus the upstream heads read
  here.

## Sources

- GitHub REST API, `repos/{owner}/{repo}` and
  `repos/{owner}/{repo}/commits/{pin}` for every row above, read 2026-11-25.
- Repository licence files read directly: `wine-mirror/wine/LICENSE`,
  `3Shain/dxmt/LICENSE`, `doitsujin/dxvk/LICENSE`,
  `HansKristian-Work/vkd3d-proton/LICENSE` and its source headers,
  `KhronosGroup/MoltenVK/LICENSE`, `FEX-Emu/FEX/LICENSE`,
  `Marvisak/VIctory/LICENSE`, `pragyaangaur/VIctory-macOS/LICENSE`,
  `mstorsjo/llvm-mingw/LICENSE.txt`, `MythicApp/Mythic/LICENSE.md`,
  `Jasonilization/Playdock/LICENSE`.
- README text read directly: FEX `Readme.md`, VIctory, VIctory-macOS,
  VKD3D-Proton, Sikarugir, Playdock.
- CodeWeavers, "Free and Open Source Software Code for CrossOver":
  <https://www.codeweavers.com/crossover/source>.
