# CrossOver evaluation: architecture, platform support and backends

Research note for the hybrid direction. It records what CodeWeavers and Apple
publish about CrossOver's runtime, its relationship to Wine, how it translates
x86-64 on Apple silicon, and which graphics backends it exposes. Everything here
is evidence class D: public documentation read on 2026-11-25, with nothing
installed, built or run. It is not a compatibility claim and not legal advice.

## Versions and platform support

| Product | Platform support stated by the vendor | Source |
| --- | --- | --- |
| CrossOver 26 (current series) | Runs on Apple silicon with Rosetta 2 translating x86-64 (stated in the CrossOver 27 post); the 26.3.0 source page is the published FOSS basis | [CrossOver 27 announcement](https://www.codeweavers.com/blog/mjohnson/2026/6/11/whats-in-and-whats-out-for-crossover-27), [CodeWeavers FOSS source page](https://www.codeweavers.com/crossover/source) |
| CrossOver 27 (announced for early 2027) | macOS Sonoma or newer, Apple silicon Macs only; 32-bit bottles no longer run; ARM64 builds for Mac and Linux | [CrossOver 27 announcement](https://www.codeweavers.com/blog/mjohnson/2026/6/11/whats-in-and-whats-out-for-crossover-27) |
| Mac ARM64 Preview (2026-07-31) | Universal build containing both ARM64 and Intel Wine. The ARM64 parts require macOS 26.5 or newer; on older systems the Intel Wine path runs under Rosetta. CodeWeavers describes these builds as testing-only and unpolished | [ARM64 preview announcement](https://www.codeweavers.com/blog/mjohnson/2026/7/31/crossover-preview-the-right-to-bear-arm64-on-mac) |

The 26 series therefore depends on Rosetta 2 today, while the 27 series replaces
that dependency with an ARM64 Wine path. The exact release date of 27 is not
stated beyond "early next year" in the June 2026 post.

## Relationship to upstream Wine

CrossOver is a proprietary product built on Wine and other open-source
components. CodeWeavers states that CrossOver Mac has targeted ARM64 since the
first M1 machines, and its ARM64 work follows upstream Wine milestones: Wine 8.0
(January 2023) added PE conversion and WoW64 thunks, Wine 9.0 (January 2024)
added native Windows ARM binaries and emulated i386 code, and Wine 10.0 (January
2025) added full ARM64EC support for emulated x86-64 code. The July 2026 ARM64
preview includes Wine 11.13 changes in the Intel build.

What CodeWeavers changes on top of those releases is **unknown**: the published
FOSS tarball is available, but no public document reviewed here enumerates
CrossOver's delta from upstream Wine. This is the same gap recorded in the
[upstream registry](upstream-registry.md).

## CPU translation

Two different translation paths exist across the 26 and 27 series:

| Path | Mechanism | Evidence |
| --- | --- | --- |
| CrossOver 26 on Apple silicon | x86-64 instructions are translated by Rosetta 2 | "Currently, CrossOver requires the use of Rosetta 2 to translate x86_64 instructions to ARM64" ([CrossOver 27 announcement](https://www.codeweavers.com/blog/mjohnson/2026/6/11/whats-in-and-whats-out-for-crossover-27)) |
| CrossOver 27 / Mac ARM64 preview | A CodeWeavers-built macOS port of FEX handles i386 and x86-64 emulation, with an ARM64 Wine build; the universal build still contains Intel Wine for older systems | [ARM64 preview announcement](https://www.codeweavers.com/blog/mjohnson/2026/7/31/crossover-preview-the-right-to-bear-arm64-on-mac) |

CodeWeavers states that Rosetta 2 will be "largely discontinued with macOS 28 in
2027" and that its ARM64 build runs without it. Apple's own notices, both read on
2026-11-25, disagree about the last macOS release that supports Rosetta:

- Apple Developer news, 1 September 2026: "macOS 27: Final release to support
  Rosetta — Intel-only apps will no longer run on Mac computers with Apple
  silicon after this update", adding that Rosetta for "older, unmaintained gaming
  titles that rely on Intel-based frameworks will continue to be supported"
  ([Apple news](https://developer.apple.com/news/?id=w5ngl9k2)).
- "App Store submissions now open for the latest OS releases", Apple Developer
  news, 9 September 2026: "macOS 26 is the final release supporting Intel Mac
  computers and Rosetta — macOS 27 will be Apple silicon only"
  ([Apple news](https://developer.apple.com/news/?id=k1mtkt1k)).

Both statements are recorded here unchanged. CodeWeavers' "macOS 28" wording is a
third date. Until Apple clarifies, support-lifetime planning cannot pick one. The
1 September exception is written for Intel-framework games and says nothing about
x86-64 Windows code run through Wine; its scope for CrossOver workloads is
**unknown**.

### Observed on an installed build

On 2026-11-25 a maintainer recorded CrossOver 26.3.0.39832 on Apple silicon: the
Wine host binaries (`wineloader`, `wineserver`) are x86_64-only and run under
Rosetta 2, the application executable is universal, Windows ARM64 PE files are
rejected as "cannot execute", and the D3DMetal, DXMT, DXVK (MoltenVK) and wined3d
backends each created a Direct3D 11_0 device on an M4. This is a recorded
observation reported on #348, not vendor documentation.

## Graphics backends and Direct3D coverage

CodeWeavers documents the following backend choices for CrossOver 26 and later:

| Backend | Basis | Direct3D coverage | Notes |
| --- | --- | --- | --- |
| Auto | Per-game entry in CodeWeavers' proprietary database, falling back to wined3d | Depends on the database entry | The database itself is not public |
| wined3d | Wine's own Direct3D implementation; the page does not state which host API it targets (inference: the host graphics API) | Not stated on the page | Used when Auto finds no entry |
| D3DMetal | Graphics API translation layer from Apple's Game Porting Toolkit | DirectX 11 and DirectX 12 | Proprietary; licence not established here (#354 records what is known) |
| DXMT | Metal-based Direct3D 11 implementation | Direct3D 11 (and 10, per the DXMT project) | Open source, LGPL-2.1-or-later; included in the ARM64 preview |
| DXVK | Vulkan-based translation layer | Direct3D 10 and 11 | Requires a working Vulkan implementation; MoltenVK is the macOS candidate named in the project registry, not on the CodeWeavers page (inference) |

Source: [Advanced Settings in CrossOver Mac 26](https://support.codeweavers.com/en_US/advanced-settings-in-crossover-mac-26).
The same page states that DLSS through MetalFX applies only to the D3DMetal and
DXMT backends.

The Mac ARM64 preview announced on 2026-07-31 has **no D3DMetal**, states that
Direct3D 12 support is "coming soon", and recommends DXMT for testing. Which
backend the 27 series will use for Direct3D 12 on ARM64 is therefore **unknown**
from the sources read.

## Open versus proprietary parts

Open-source components named by CodeWeavers or their own projects include Wine
(LGPL-2.1-or-later), DXMT (LGPL-2.1-or-later), DXVK (zlib), MoltenVK
(Apache-2.0) and FEX (MIT in its own repository). Proprietary or
terms-unestablished parts include the CrossOver client and product, D3DMetal and
Rosetta 2.

The CodeWeavers FOSS source page for 26.3.0 lists Wine, Vkd3d (Wine's), DXVK and
MoltenVK; it does not list DXMT or FEX. DXMT appears only as a backend on the
support page and FEX only in the ARM64 preview post, so whether CrossOver offers
source for the DXMT and FEX builds it ships is **unknown**. Licences and
redistribution boundaries are recorded separately in [the upstream
registry](upstream-registry.md) and in issue #354; this note does not decide what
may be bundled.

## Unknowns

- CrossOver's exact delta from the upstream Wine release it is based on.
- Whether CrossOver ships DXMT and FEX, at which revisions, and whether their
  source is offered; the FOSS Appendix A was not located.
- The final CrossOver 27 release date and its complete backend matrix.
- Which Direct3D 12 path the ARM64 build will support once D3DMetal is added.
- Whether the FEX-based path covers every instruction form the 26/Rosetta path
  handles; CodeWeavers calls the preview unpolished and lists broken launchers.
- Rosetta 2's last supported macOS release, because Apple's two notices conflict,
  and whether Apple's exception for older games covers Windows code under Wine.
- Any Xbox runtime, system-service or title compatibility: CrossOver provides
  Windows compatibility, and nothing read here extends it to Xbox software.

## Sources

- CodeWeavers, "What's in and what's out for CrossOver 27", 11 June 2026:
  <https://www.codeweavers.com/blog/mjohnson/2026/6/11/whats-in-and-whats-out-for-crossover-27>
  (archived copy:
  <https://web.archive.org/web/20260815045035/https://www.codeweavers.com/blog/mjohnson/2026/6/11/whats-in-and-whats-out-for-crossover-27>)
- CodeWeavers, "CrossOver Preview: The right to bear ARM64 on Mac", 31 July
  2026:
  <https://www.codeweavers.com/blog/mjohnson/2026/7/31/crossover-preview-the-right-to-bear-arm64-on-mac>
  (archived copy:
  <https://web.archive.org/web/20260827065551/https://www.codeweavers.com/blog/mjohnson/2026/7/31/crossover-preview-the-right-to-bear-arm64-on-mac>)
- CodeWeavers Support, "Advanced Settings in CrossOver Mac 26":
  <https://support.codeweavers.com/en_US/advanced-settings-in-crossover-mac-26>
- CodeWeavers, "Free and Open Source Software Code for CrossOver" (26.3.0
  tarball and component list): <https://www.codeweavers.com/crossover/source>
- Apple Developer News, "Upcoming changes to Rosetta support for Intel-based
  macOS apps", 1 September 2026:
  <https://developer.apple.com/news/?id=w5ngl9k2>
- Apple Developer News, "App Store submissions now open for the latest OS
  releases", 9 September 2026:
  <https://developer.apple.com/news/?id=k1mtkt1k>
- Project records: [upstream registry](upstream-registry.md),
  [primary-source verification](primary-source-verification-2026-11-25.md).

All pages were read on 2026-11-25. The two CodeWeavers blog posts returned an
automated-fetch 403 and were read in a browser instead.
