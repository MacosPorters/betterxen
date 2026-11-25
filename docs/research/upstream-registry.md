# Upstream research registry

Reviewed 2026-11-25. Every entry is **evidence class D** (non-normative official or project material) in the sense used by the issue templates and [claims.toml](../claims.toml): a review of public documentation and repository metadata. Nothing here was installed, built or run, so none of it is a statement that a project works for BetterXen's goals. Licenses are what the repository's own license file or GitHub's license detection reported; they still need the per-file check in the [reuse matrix](component-reuse-matrix.md) and are not legal advice. Pins are the default-branch head observed with `git ls-remote` on the review date. Independent re-derivation of this table is tracked in issue #363.

| Project | Source | Pin (head) | License as reported | Role for BetterXen | Findings |
| --- | --- | --- | --- | --- | --- |
| CrossOver (CodeWeavers) | [source page](https://www.codeweavers.com/crossover/source) | CrossOver 26.3.0 source listed | Proprietary product; page lists GPL/LGPL and other FOSS components it uses | Primary compatibility foundation (external install) | The page offers FOSS source for CrossOver 26.3.0 and lists Wine, Vkd3d, DXVK, MoltenVK, FAudio, SDL, LLVM, GMP and others. It says users should prefer each project's own home page. D3DMetal and Rosetta 2 are not in that list. What else is proprietary, the supported integration interfaces and redistribution terms are **unknown** (#348, #350, #354). |
| Wine | [wine-mirror/wine](https://github.com/wine-mirror/wine) | `a79470449d03` | LGPL 2.1 (repository LICENSE and COPYING.LIB); GitHub reports NOASSERTION | Runtime APIs, via CrossOver | Upstream of CrossOver's runtime. CrossOver's exact delta from upstream is not reviewed. |
| DXMT | [3Shain/dxmt](https://github.com/3Shain/dxmt) | `faa73e4a0686` | LGPL, version 2.1 or any later version (LICENSE text); copyright notice "for CodeWeavers" | Direct3D 11/10 to Metal under Wine; candidate backend | README: Metal-based D3D11/D3D10 layer for macOS using Wine. Status is on the project wiki (not reviewed). D3D12 is not claimed. |
| DXVK | [doitsujin/dxvk](https://github.com/doitsujin/dxvk) | `57b2a7c1e87b` | zlib/libpng (LICENSE; GitHub: Zlib) | Direct3D to Vulkan; candidate backend only with MoltenVK | Listed among CrossOver's FOSS components. Whether it is used for any macOS path is unknown. |
| VKD3D-Proton | [HansKristian-Work/vkd3d-proton](https://github.com/HansKristian-Work/vkd3d-proton) | `71cae091b065` | LGPL 2.1 (GitHub) | Direct3D 12 to Vulkan; candidate for evaluation | README requires Vulkan 1.3 and modern extensions. Whether MoltenVK exposes what it needs is **unknown**. CrossOver's page lists "Vkd3d" from source.winehq.org, which is the Wine vkd3d, not necessarily this fork. |
| MoltenVK | [KhronosGroup/MoltenVK](https://github.com/KhronosGroup/MoltenVK) | `5189f8de86ec` | Apache-2.0 | Vulkan on Metal; candidate under DXVK/VKD3D-Proton | A Vulkan Portability layer, a subset of Vulkan. Listed among CrossOver's FOSS components. Last push observed 2026-10-13. |
| FEX | [FEX-Emu/FEX](https://github.com/FEX-Emu/FEX) | `3ee25399232a` | MIT (GitHub) | x86/x86-64 emulation reference | README describes ARM64 **Linux**; tested distributions listed are Linux. No macOS host support is stated. Not a macOS option on this evidence; useful as design reference. |
| VIctory | [Marvisak/VIctory](https://github.com/Marvisak/VIctory) | `13f91caee25f` | GPL-3.0 (GitHub); a prior local review found per-file notices (see `experiments/victory-native/licenses.md`, local, untracked) | Discovery reference only | Described as an attempt at running one commercial game on PC. Its inputs are an Xbox Series build of that game. PortWatch describes the efforts it tracks as targeting a "leaked Xbox Series build". |
| VIctory-macOS | [pragyaangaur/VIctory-macOS](https://github.com/pragyaangaur/VIctory-macOS) | `485b230ef596` | GPL-3.0 (GitHub) | Discovery reference only | A macOS port of VIctory using Rosetta, CrossOver Wine and D3DMetal per its README (also noted in [portwatch-native-macos.md](portwatch-native-macos.md)). Same input concern. Its test and "works" claims are self-reported and not verified here. The repository has no parent fork, so the "canonical upstream" is VIctory. |
| PortWatch VI | [portwatch-vi.xyz](https://portwatch-vi.xyz/), [4lwayz/portwatch-vi](https://github.com/4lwayz/portwatch-vi) | `ed00263cc0c9` | NOASSERTION | Project discovery reference | A tracker of community efforts; progress figures are editorial, with self-reported claims labelled. Not engineering evidence; flagged and ignored queues are not endorsed. |

## Additional candidates found by search (not reviewed)

Found by GitHub search on 2026-11-25. Only repository metadata was read; none was evaluated, installed or run, and none is a recommendation. Licenses are what GitHub reported.

| Project | Source | Pin (head) | License as reported | Why listed |
| --- | --- | --- | --- | --- |
| llvm-mingw | [mstorsjo/llvm-mingw](https://github.com/mstorsjo/llvm-mingw) | `903f4cf7faa2` | GitHub: other (read LICENSE before use) | Cross-compiler candidate for building original Windows test programs (#364). README says its compilers come in x86_64 and aarch64 builds, built on Ubuntu; availability and behaviour on a macOS host are **unknown**. |
| Sikarugir | [Sikarugir-App/Sikarugir](https://github.com/Sikarugir-App/Sikarugir) | `5446b7d18072` | not reported | Wine-wrapper project (successor to Wineskin per its README, macOS 14.6+): a reference for bottle and wrapper management. |
| macOS Wine builds | [Gcenx/macOS_Wine_builds](https://github.com/Gcenx/macOS_Wine_builds) | `72f8915f2d01` | not reported | Community Wine builds for macOS: possible Wine reference builds. Provenance of the binaries is unreviewed. |
| Mythic | [MythicApp/Mythic](https://github.com/MythicApp/Mythic) | `754223978e61` | GPL-3.0 | Open-source macOS launcher that runs Windows games through Wine: an architecture reference for configuration and launching. |
| Playdock | [Jasonilization/Playdock](https://github.com/Jasonilization/Playdock) | `f58b8ef7fb70` | MIT | Describes itself as a free alternative to CrossOver; unreviewed. |

## Not yet researched

Apple's Game Porting Toolkit documentation and license (D3DMetal's actual terms), Rosetta 2's support lifetime, CrossOver's documented command-line interfaces and bottle management, and other established projects. These are issues #349, #350 and #354; nothing is claimed about them here.

## Provenance decision

VIctory-family projects are not adopted. Their GPL grants apply to their own source; they do not establish that the material they were built for is authorized. The registry records them for architecture ideas visible in public documentation (cache identity, upload visibility, test structure). No code, shader corpus, asset or tooling from them enters BetterXen without a recorded per-file review, and anything game-specific or decryption-related is excluded.
