# PortWatch review and a native macOS execution plan

Reviewed November 24, 2026 (America/Chicago). This is a public-documentation and
source-boundary review, not a competitor execution campaign or independent
acceptance. **BetterXen does not support commercial Xbox game execution.**

## Recommendation

Continue BetterXen as an original ARM64 runtime with its own x86-64 CPU
translation and direct Metal backend. The Windows projects expose useful
engineering questions, test structures and failure categories. Their code is
not a drop-in native port. Do not fork a Windows emulator into this repository,
replace the CPU with Rosetta, or add a Vulkan/Windows compatibility runtime.

The next useful result is an original compiled application that starts through
a checked module/runtime path and reaches a translated, checked Metal frame.
Broader vector instructions, thread lifecycle, binding metadata and console
contracts must advance through explicit fixture-backed increments. Completing
synthetic applications alone cannot prove Xbox compatibility.

## What was inspected

The tracker snapshot is [PortWatch 2cedf62a](https://github.com/4lwayz/portwatch-vi/blob/2cedf62a16b63697f096763ecb99b4c10d4dbccc/data/tracker.json),
whose update timestamp is `2026-11-25T04:22:56.289Z` (November 24 locally).
Its 35 efforts, 22 tools and 30 candidates were screened. One candidate repeats
XCL. Three efforts have no public repository; seven repository endpoints were
unavailable across the combined list. Available repositories were pinned to
their observed default-branch commit; their metadata, README and public tree
were used to classify target, host assumptions, lineage and evidence scope.
The per-project tables below include every entry in these three lists: 85
unique repository endpoints were checked, 78 available and seven unavailable.

This is not a line-by-line audit of every source file, proof of all contributor
provenance, or a claim to understand unavailable/private code. No upstream
project, setup script, Windows executable, game, shader corpus, asset collection,
restricted SDK or decryption tool was run. Archive and protection-related entries
were screened at metadata/README level only; no mirrored implementation or
restricted payload was recovered. The tracker's flagged/ignored discovery/spam
queues are not endorsed engineering candidates and were not imported.

Selected public architecture/status documentation was read more deeply for
VIctory, VIctory-macOS, TuxBox, NativeX, SeriesX and XValkyrie; XSXEmu's README
provides detailed admission, TLS and graphics boundaries. Those documents
explain their own designs; they do not verify their claimed test/hardware results.
The supplied `/so` website URL was unavailable to the web reader; the public,
pinned repository data was accessible and matched the initial screened JSON.

BetterXen's working tree started at `f02eb2678e76e91876be5706ce91dca25dcc3017`
with existing local documentation edits. Read-only `git fetch` found newer main
[`b2654e93`](https://github.com/MacosPorters/betterxen/tree/b2654e93a42e74e29570183da1f31bb8cdc6f721).
A separate ignored source snapshot was inspected without switching, resetting
or merging the working tree. This distinction changes the hosted-C status.
Local documentation edits from this review do not install that newer runtime.

## Why the PC results do not transfer directly

| Layer | Common PC approach described upstream | Native ARM64 BetterXen requirement |
| --- | --- | --- |
| CPU | Guest x86-64 runs on a matching x86-64 CPU; several projects require AVX2 | Decode/execute or translate each supported form, preserve precise state/faults, and advertise only satisfied guest features. |
| Loader/runtime | Windows APIs or Wine/Proton supply much of PE startup, TLS, exceptions, threads, files and CRT behavior | Original checked guest memory, module lifecycle, service implementations, callback bridges and guest exception design. |
| Graphics API | Console-facing objects dispatch onto desktop D3D12, sometimes through Vulkan | Explicit validated guest handles/commands and native Metal objects, lifetime, resource state and completion. |
| Shaders | RDNA machine code becomes HLSL/DXIL or SPIR-V | Independently verified RDNA-to-IR-to-MSL behavior, entry state and resource bindings. SPIR-V may be an intermediate, not a Vulkan runtime requirement. |
| Assets | Title supplies its own loaders or tools convert game-specific data | No need to become an archive viewer to prove guest execution; codecs/layouts require public contracts and demonstrated workload needs. |
| Evidence | README milestones, streams, screenshots, compilation counts or self-reported FPS | Pinned original fixtures, independent expectations, actual native execution/readback and candidate-specific receipts. |

PortWatch is a discovery/editorial service. Its percentages are not compatibility
measurements. The observed entries label their verification self-reported;
phrases such as “confirmed” inside editorial summaries do not become BetterXen
acceptance. Several forks share VIctory/XCL/Leonida lineage: ten forks are not
ten independent confirmations. Compilation, shader validation, first pixels,
scene completeness, interactive progress and stable play are distinct outcomes.

## macOS efforts and what “native” means here

[VIctory-macOS edba53dd](https://github.com/pragyaangaur/VIctory-macOS/blob/edba53ddc2207e6c537679f4ce06f966b7a63a59/README.md)
uses Rosetta, CrossOver Wine and D3DMetal for the guest route. Native builds of
its portable crates are a different result. Its reported passing tests neither
establish a fully ARM64 runtime nor demonstrate playable game execution here.
Its [port notes](https://github.com/pragyaangaur/VIctory-macOS/blob/edba53ddc2207e6c537679f4ce06f966b7a63a59/docs/macos.md)
provide leads for cache identity, upload visibility, floating-point edge cases,
capability/behavior mismatches and masked-store page faults. Reproduce relevant
questions with original Metal/CPU fixtures; do not copy its workarounds or assume
a D3DMetal defect exists in BetterXen's direct Metal path.

[XSXEmu 2d937f83](https://github.com/daeken/XSXEmu/blob/2d937f8360fbcdac368a0e363c24df322fa24db2/README.md)
is another macOS effort, so “nobody is working on Mac” would be inaccurate.
Its guest path uses an x86-64 Mach-O process and Rosetta, while owned shader
translation fixtures use Metal. It explicitly separates static audit from
execution admission and says no game frame has been produced. Its useful
lesson is that entry/binding information and DLL/TLS lifecycle are separate
contracts; its Rosetta guest route does not satisfy BetterXen's ARM64 rule.

[6stars 480d9a4d](https://github.com/meetox80/6stars/blob/480d9a4d00ed11560f7b4d989643723a0f5c451c/README.md)
and the separately listed 6stars engine candidate describe Swift/Metal world-data
rendering. An independent renderer may produce a convincing scene while never
executing the original game program. That is valuable work with a different
scope. No verified reusable component or file-level license clearance was
established by this review.

[nativ64 3b826108](https://github.com/Ybucaille/nativ64/blob/3b82610882bc55cb27f0ec563c126a226855a8ea/README.md)
targets Linux x86-64. Its central observation is relevant: host Windows APIs
that a Windows project forwards still need implementation on a different OS.
Its Linux syscalls/futexes and x64 ABI are not native Darwin ARM64 services.
BetterXen should prioritize from its own import/fixture failures, not copy that
project's title-specific API totals or implementation.

## BetterXen's actual starting point

| Area | Observed on current main b2654e93 | Consequence |
| --- | --- | --- |
| CPU inventory | 498 declared forms, 213 operations; tool classifies 93 operations native and 120 shared-semantics | These are inventory/lowering categories, not 498 independently accepted instructions or a compatibility percentage. VEX remains rejected. |
| Typed runtime registry | 47 services: 7 implemented, 37 partial, 3 stubs; tool reports 0 verified entries | This is the typed subset, not the total handwritten runtime. Named exports alone do not prove behavior. |
| Hosted C | A1 and A2 are present; PR #283 merged as d26af53c. Gate A records local passes awaiting independent review | Do not open another “implement hosted C” issue. Reproduce/accept the candidate and expand from a failing original workload. |
| Modules | startup.rs accepts one image; modules.rs exposes built-in service modules | Built-in LoadLibrary/GetProcAddress is not general EXE plus guest-DLL startup and teardown. |
| Threads | Deterministic serial guest scheduler with worker stacks/TEBs, waits and handles | Sleep can stall all workers; static worker TLS and thread/FLS callbacks remain documented gaps. Serial scheduling does not prove parallel x86 memory ordering. |
| Graphics | Translated image_sample_lz pixel stage, native texture tests and guest descriptor-table bindings already exist | README X10 and generic “no sampling” descriptions are stale for this bounded path. Broader bindings, control flow and sampling still need contracts. |
| Shader inventory | Table records 1,465 identified forms, 189 decoded, 185 IR/MSL, 18 accepted Metal forms; reconciliation finds zero mismatches | Different domains and acceptance stages must stay separate. The compute inventory does not fully describe graphics-only sampling. #225 owns pending acceptance. |
| Texture/resource work | Packed BC footprints and bounded private texture staging merged; original resource/draw tests exist | Reuse them. Do not duplicate #260's active format inventory or assume guest/console integration from host upload success. |
| Native app | Existing Swift/FFI fixtures; app/CI and usability PRs remain separate | UI/audio/input fixtures are not one integrated compiled guest application. #256 is the existing integration task. |
| Console contract | Public AMD desktop ISA/PE/D3D12 documentation and explicit synthetic ABIs | Authentic launch, descriptors/commands and console-specific behavior remain unknown or unverified. No Xbox hardware access exists. |

Evidence for these boundaries: current-main
[CPU registry](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/docs/cpu/instruction-registry.md),
[Gate A](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/docs/testing/gate-a.md),
[startup](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/crates/loader/src/startup.rs),
[module services](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/crates/loader/src/services/modules.rs),
[threads](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/docs/runtime/threads.md),
[graphics compiler](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/crates/shader/src/graphics.rs),
and [semantic coverage](https://github.com/MacosPorters/betterxen/blob/b2654e93a42e74e29570183da1f31bb8cdc6f721/docs/shaders/semantic-coverage.md).
These are source/inventory observations. Historical hardware results retain
their existing class A scope; none was rerun or independently certified here.

## What to take forward

Take **questions and test designs**, then implement independently from primary
sources. No emulator source was copied, ported, translated, vendored or adapted.
Repository-level license labels below are metadata only. They do not establish
per-file grants, GPL-3.0-or-later compatibility or rights to fixtures/data.

1. Build a small, falsifiable workload before expanding an entire subsystem.
   TuxBox/NativeX-style separation of loader, ABI, GPU and presentation tests is
   useful; their own expected outputs cannot become BetterXen's oracle.
2. Treat unresolved host services as a concrete inventory. Reuse #211's package
   census, #254's report schema and #255's startup diagnostics. Keep unsupported
   calls explicit; never return success merely to get farther.
3. Model CPU state and faults before optimizing translation. AVX/YMM, masked
   memory accesses, unwinding and lifecycle cannot be supplied by ARM64 host
   calling conventions or a renamed Windows DLL.
4. Make shader entry state, resources and pipeline identity explicit. A shader
   can compile and still read the wrong descriptor or run under the wrong mask.
   A replaced/skipped shader or an empty draw cannot pass a correctness gate.
5. Validate resource ownership and completion before pursuing zero-copy. Apple
   unified memory does not remove guest/host address translation, storage modes,
   alignment, cache visibility or in-flight lifetime requirements.
6. Expand wave behavior, loops, mesh and ray tracing only as separate reviewed
   contracts. A first simple scene does not require all of them; future console
   workloads may. Do not infer optional Metal features from a product name.
7. Measure startup/steady-state and correctness separately, with actual host
   receipts. Cache keys must cover full relevant identity; prewarming is an
   experiment under #258, not a prerequisite or promised performance win.

Primary semantic authorities are the [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html),
[public PE format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format),
[Microsoft x64 exception handling](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64),
[AMD ISA publications](https://gpuopen.com/amd-gpu-architecture-programming-documentation/),
[public resource-binding specification](https://microsoft.github.io/DirectX-Specs/d3d/ResourceBinding.html),
and [Apple Metal feature tables](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf).
Class B applies only to what the relevant public specification documents;
applying desktop behavior to Xbox needs separate evidence (often C or U).
Feature tables are documentation, not our execution evidence across M1–M5.

## Contributor work and dependencies

The review is tracked in [#298](https://github.com/MacosPorters/betterxen/issues/298).
The new tasks below have bounded deliverables, proposed paths, public references,
negative controls and acceptance limits. They are unclaimed implementation work.
Design tasks can begin with public documentation; runtime changes must respect
parent claims and shared-file ownership. Read every issue comment and linked PR
before claiming. Do not claim several tasks in one comment.

| Priority/order | Work | Dependency and boundary |
| --- | --- | --- |
| First, research | [#299 AVX/AVX2 state and decode contract](https://github.com/MacosPorters/betterxen/issues/299) | Extends beyond existing #120–#124; keeps CPUID honest. No blanket AVX2 implementation claim. |
| First, research | [#300 bounded AMD64 unwind](https://github.com/MacosPorters/betterxen/issues/300) | Separates metadata/stack walking from SEH and C++ exceptions; existing trap stubs remain. |
| First, loader contract/fixtures | [#301 original EXE plus two-DLL startup](https://github.com/MacosPorters/betterxen/issues/301) | Reuses #211 census and supplies a prerequisite to #256. Production mapping changes are separately coordinated. |
| First, GPU contract | [#302 shader entry and resource bindings](https://github.com/MacosPorters/betterxen/issues/302) | Builds on existing #226 sampling/table work; coordinates #205, #179, #180 and #224. |
| First, QA design | [#303 native capability receipts](https://github.com/MacosPorters/betterxen/issues/303) | Reuses #254 schema, #248 app CI and #158 dependency evidence. No automatic fleet posting. |
| Next, scheduler implementation | [#304 Sleep yields ready workers](https://github.com/MacosPorters/betterxen/issues/304) | Narrow #228 child; coordinate parent and runtime PRs before code edits. |
| First, lifecycle design | [#305 worker static TLS and callbacks](https://github.com/MacosPorters/betterxen/issues/305) | Depends on module/scheduler contracts for production; no competing scheduler. |
| Parallel, public research | [#306 console launch/graphics evidence ledger](https://github.com/MacosPorters/betterxen/issues/306) | Links existing public research; unknown console details remain blockers rather than guesses. |

Reuse existing tasks rather than adding duplicates: #179 runtime uniform
branches, #180 bounded loops, #205 semantic expansion, #225 independent shader
acceptance, #224 fences, #147 resource-update workload, #148 shared lifetime,
#256 startup-to-frame integration, #260 format inventory, #247 quick start,
#250 session errors, #254 compatibility reports and #255 worker diagnostics.
PRs #293–#297 already cover app CI, CPU coverage, selection ordering, Open and
texture inventory respectively. Runtime PRs #263/#290 overlap substantially;
their author/maintainer must resolve them before new runtime edits. These
references are destination-repository numbers, not imported source numbers.

The replacement [development phases](../development-phases.md) define the sole
active work order, from compiled guest boot through modular workers, a
guest-controlled Metal scene, an interactive application, a compatibility
corpus, sustained native execution and conditional Xbox experiments.
Hosted C is implemented on inspected main, awaiting independent acceptance;
it is no longer an untouched prerequisite. Original integration, then measured
optimization, then only evidence-admissible compatibility experiments is the
path. There is no supported date or estimate for running a commercial game.

## Complete project screen

Each pin links to the exact README reviewed. “Unavailable” means that the
observed public GitHub endpoint returned 404; it does not prove why or justify
looking for restricted mirrors. “Unresolved” licenses and NOASSERTION are not
reuse permission. Descriptions below classify documentation, not executed code.

### Tracker efforts (35)

| Entry | Pinned public documentation | Classification and BetterXen decision |
| --- | --- | --- |
| ZETTROX (`Zettrox/ztrx`) | [`90b529f8`](https://github.com/Zettrox/ztrx/blob/90b529f878a470fec9bf07b7be316bc769a12232/README.md); license metadata: `GPL-3.0` | VIctory/victory-fork lineage; Windows x64 + D3D12. Hardware tuning and reported gameplay are not independent native-Mac evidence. |
| Uilian (`uiliannn/VItoriaBr`) | [`c2485475`](https://github.com/uiliannn/VItoriaBr/blob/c24854750132af24c10e954765047525e0249bd3/README.md); license metadata: `GPL-3.0` | VIctory-family Windows/DXIL fork; README and tracker differ on readiness. Keep results self-reported; no ARM64 execution path. |
| suri | No public repository in tracker | Private/stream report only; no implementation inspection or reproducibility. No source/fixture reuse proposed. |
| omonkey | No public repository in tracker | Private/stream report only; no implementation inspection or reproducibility. No source/fixture reuse proposed. |
| CGBJ | No public repository in tracker | Private/stream report only; no implementation inspection or reproducibility. No source/fixture reuse proposed. |
| hyphennn (`fxceless/victory-fork`) | [`310c6eb6`](https://github.com/fxceless/victory-fork/blob/310c6eb6c16822ab95076aad61fce19bba86bb98/README.md); license metadata: `GPL-3.0` | VIctory rendering fork, Windows x64/D3D12; useful failure categories include bindings and mesh work. No implementation reuse. |
| VIctory (`Marvisak/VIctory`) | [`940d32f6`](https://github.com/Marvisak/VIctory/blob/940d32f69b22265690a0b5a15d6ded572ba33baf/README.md); license metadata: `GPL-3.0` | Windows x64 direct guest execution, HLE and D3D12/DXIL. Public architecture documents host pass-through and title-specific work. |
| xbox-emulator (`omonkeyz/xbox-emulator`) | [`f89e4106`](https://github.com/omonkeyz/xbox-emulator/blob/f89e4106fe8a1728515afa34ceb5097132cadcda/README.md); license metadata: `GPL-3.0` | TomBox continuation; Windows-process runtime and RDNA shader work. README title reports are unverified here; assets/captures are excluded. |
| XCL (`xcl-dev/xcl`) | [`f5c5bdf0`](https://github.com/xcl-dev/xcl/blob/f5c5bdf0e91035f8f1404f46f4f2f7aedc259e7f/README.md); license metadata: `GPL-2.0` | Windows x64 direct execution plus D3D12/DXIL; host services and pipeline behavior matter. GPL-2.0-only README; no reuse clearance. |
| ScarlettX (`Freminet/ScarlettX`) | [`01c642cf`](https://github.com/Freminet/ScarlettX/blob/01c642cf62a6d9978f535e2c438c488ebcb4cd39/README.md); license metadata: `unresolved` | Windows x64 HLE/D3D12; separates console precompiled-shader gaps from desktop paths. Console-binary-derived findings need independent public evidence. |
| TuxBox (`brcly/TuxBox`) | [`97b8ad31`](https://github.com/brcly/TuxBox/blob/97b8ad3192def18d491fe72ea3a55e204bc77a5d/README.md); license metadata: `Apache-2.0` | x86-64 Windows and Linux/Proton; owned guest fixtures and provider-specific failures are useful testing patterns, not an ARM64 solution. |
| americas-recomp (`realdody/americas-recomp`) | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| PE-Vulkan project (`ItsNotRexed/Leonida`) | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| OpenScarlett (`californiaAlmonds/OpenScarlett`) | [`022a8b25`](https://github.com/californiaAlmonds/OpenScarlett/blob/022a8b25148cc08412df8ac7e752b9efed4cd819/README.md); license metadata: `MIT` | Windows x64, D3D12/Vulkan; broad completeness claims are upstream reports. Public-spec-backed original fixtures are the only proposed transfer. |
| scarlett-layer (`jadeowaowa/scarlett-layer`) | [`0e436b3e`](https://github.com/jadeowaowa/scarlett-layer/blob/0e436b3e1c654765b193c1cb705918fbf101b10b/README.md); license metadata: `Apache-2.0` | Windows import interception/shims, D3D12 and synthetic test tiers; useful distinction between desktop samples and console compatibility. |
| Xbox2Windows (`R0Y00/Xbox2Windows`) | [`ab9a020c`](https://github.com/R0Y00/Xbox2Windows/blob/ab9a020c0783f9ce1dd10876cbeb119707b9ada9/README.md); license metadata: `MIT` | x64-to-C++ AOT/loader and DXIL research; distinguishes modified diagnostic startup from original executable compatibility. Host ABI still differs. |
| AnyXboxX (`LooperSalty/AnyXboxX`) | [`b92a71b9`](https://github.com/LooperSalty/AnyXboxX/blob/b92a71b9308e76f8ec9059c01d6e18f21611dc20/README.md); license metadata: `GPL-3.0` | Unicorn research and native Windows routes; current README also uses VIctory lineage. Do not treat its older interpreter label as ARM64 readiness. |
| XValkyrie (`sammwyy/xvalkyrie`) | [`c4c04970`](https://github.com/sammwyy/xvalkyrie/blob/c4c04970930d029a25622743e3ff4ef873ac17a6/README.md); license metadata: `GPL-3.0` | Windows x64 console API and shader translation; useful subsystem boundaries. Console/title-specific ABI claims remain unverified. |
| GTAVI-Recomp (`JamesCicada/GTAVI-Recomp`) | [`71766d8b`](https://github.com/JamesCicada/GTAVI-Recomp/blob/71766d8b5c669bc44639ad85bfd84edd8337c75a/README.md); license metadata: `GPL-2.0` | PS5/AnyPS5 route, not an Xbox launch contract; do not adapt emulator source or transfer PS5 APIs to Series. |
| AnyXboxSeries (`edupode/anyxboxseries`) | [`9ca2d053`](https://github.com/edupode/anyxboxseries/blob/9ca2d05329fe878db6abea6060889f9a95566719/README.md); license metadata: `NOASSERTION` | Windows D3D12 loader/inspection and an original sample; sample presentation does not establish Xbox guest execution. |
| SCARAB (`p-portela/SCARAB`) | [`8140e0be`](https://github.com/p-portela/SCARAB/blob/8140e0be4908499a5c4381eb4532410f05e39369/README.md); license metadata: `GPL-3.0` | Windows/D3D12 bridge, Linux through vkd3d-proton; memory/presentation incompleteness is explicit. Not native ARM64. |
| AnyBox (`Dark3581/AnyBox`) | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| PercyScarlett (`rpatty-dev/PercyScarlett`) | [`64719f96`](https://github.com/rpatty-dev/PercyScarlett/blob/64719f96d32f74d9d68cec69243b515ba675741f/README.md); license metadata: `GPL-2.0` | Windows x64/D3D12; README distinguishes existing DXIL titles from RDNA lifting and reports instability. GPL-2.0-only; no reuse. |
| GrandTheftXbox (`zohaiblazuli/GrandTheftXbox`) | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| Scarlett Windows Compat (`ProhorovIlja/NativeX-Runtime`) | [`b3087ce0`](https://github.com/ProhorovIlja/NativeX-Runtime/blob/b3087ce0ab191227dc9e7af37cab97a0b097bb98/README.md); license metadata: `MIT` | Portable inspection and separate Windows/Linux ABI/graphics probes. Useful test decomposition; not a complete application runtime. |
| DedSec (`Hylanderry/DedSec`) | [`c44b38ca`](https://github.com/Hylanderry/DedSec/blob/c44b38ca0207900cd27c687bb9213a6c8474bc69/README.md); license metadata: `MIT` | Windows x64/direct execution and D3D12; performance/feature claims need reproducible evidence. No Mac execution established. |
| Xbgtasix (`ljm-233/Xbgtasix`) | [`1a767872`](https://github.com/ljm-233/Xbgtasix/blob/1a7678729cd436f9adaa9772b76d97af3d292321/README.md); license metadata: `GPL-3.0` | Windows compatibility DLLs rely on the Windows loader for imports, relocation and TLS. This omitted host work is required on Darwin. |
| SeriesX Emulator (`anrq0/SeriesX-Emulator`) | [`4f07d76f`](https://github.com/anrq0/SeriesX-Emulator/blob/4f07d76f9f641ce85ce4238081c06d1cef1b7521/README.md); license metadata: `MIT` | Interpreter, original test applications and desktop launcher; README explicitly excludes commercial compatibility. Reported vectors are not our oracle. |
| GTA6-COMPAT-LAYER (ScarlettCompat) (`SSTGxf/ScarlettCompat`) | [`b92f060b`](https://github.com/SSTGxf/ScarlettCompat/blob/b92f060b1754580338dc30f57bb50777af38749d/README.md); license metadata: `MIT` | Windows native loader and console shims. Diagnostic bootstrap is a useful stage model; passing imports is not an implemented API. |
| XlaunchVI (`JPTuba9/xlaunch-vi`) | [`40feebd7`](https://github.com/JPTuba9/xlaunch-vi/blob/40feebd73a59c13ac5d65c3e61d21906111294e7/README.md); license metadata: `unresolved` | Windows x64, D3D12/Vulkan and frontend. README explicitly denies game compatibility; UI work is separate from guest execution. |
| OpenScarlett (Community Edition) (`OpenCommunityEdition/OpenScarlett`) | [`d1285fdd`](https://github.com/OpenCommunityEdition/OpenScarlett/blob/d1285fddbe399927581a772f536239b33a1e2cf4/README.md); license metadata: `AGPL-3.0` | Windows/Linux API layer; README build requires GDKX. Do not follow restricted tooling dependencies or import its sources. |
| Five Star Recomp (`ThatRecompDev/five-star-recomp`) | [`fb3b3a2d`](https://github.com/ThatRecompDev/five-star-recomp/blob/fb3b3a2def5af6eef6004c954c11f2cd22574168/README.md); license metadata: `GPL-3.0` | Windows platform, shader and asset tools; title startup claims do not establish a complete scene or native Mac execution. |
| G6Recomp (`Kajcsi/G6Recomp`) | [`82ccaa5d`](https://github.com/Kajcsi/G6Recomp/blob/82ccaa5d4b3fdc1edb9cdd95c8dbab4ee7cde352/README.md); license metadata: `GPL-3.0` | Windows AVX2/direct-execution loader with fixture tests; static-recompilation name does not remove reliance on x64/Windows ABI. |
| GTA6-BootTest (`OnlyMoisties/GTA6-BootTest`) | [`32683d8f`](https://github.com/OnlyMoisties/GTA6-BootTest/blob/32683d8fe2841bddeb0b437109e97bc32cf55724/README.md); license metadata: `unresolved` | Windows debugger/shim/GPU startup harness. Reported first UI frame is not full scene correctness; no root license identified by metadata. |
| Palmetto (`luizwidmer/palmetto`) | [`6416a273`](https://github.com/luizwidmer/palmetto/blob/6416a273336599341dfd31e64cc3da3fd1b855ed/README.md); license metadata: `GPL-3.0` | Windows runtime and RDNA-to-SPIR-V/Vulkan path; behavior-stage reporting is useful, runtime/backend is incompatible with our target. |

### Tracker tools (22)

| Entry | Pinned public documentation | Classification and BetterXen decision |
| --- | --- | --- |
| `lynxnathan/rage-tools-vi` | [`547ee9ba`](https://github.com/lynxnathan/rage-tools-vi/blob/547ee9ba2dde236644a597cef65a18f7bd5549be/README.md); license metadata: `MIT` | Asset/catalog tooling; README mentions executable-derived information. No derived corpus imported; not a CPU/runtime shortcut. |
| `fishingpvalues/gta6-pc-port-hub` | [`3a7d8d18`](https://github.com/fishingpvalues/gta6-pc-port-hub/blob/3a7d8d18f6fdc2071f8609f35153fb07ac25d1b5/README.md); license metadata: `CC0-1.0` | Archived coordination hub. Its missing-executable snapshot is historical, not evidence of current file availability or native compatibility. |
| `holdmysocks/SixNG` | [`8897a1f6`](https://github.com/holdmysocks/SixNG/blob/8897a1f60c6700553987954f567d9146f898d882/README.md); license metadata: `GPL-3.0` | Asset exporter with explicit texture fallback limitations; fallback pixels must never become BetterXen correctness evidence. Defer format work to #260. |
| `rpatty-dev/PercyStar` | [`ca4cd4c0`](https://github.com/rpatty-dev/PercyStar/blob/ca4cd4c09bb3a4e29cc2d33e69385725c5fb2220/README.md); license metadata: `NOASSERTION` | Asset/shader/media tools with custom attribution terms. macOS tooling does not establish guest execution or GPL-compatible reuse. |
| `breadbbbb/rpf9-explorer` | [`cab9bb6e`](https://github.com/breadbbbb/rpf9-explorer/blob/cab9bb6ebe6d46fec6ac90ee5004bdd8cfd9e31a/README.md); license metadata: `unresolved` | Cross-platform asset viewer/exporter; root grant unresolved in metadata. Exclude code/assets; sample viewing is not guest execution. |
| `Ahmet-Dedeler/bcpack-rs` | [`b1c3099a`](https://github.com/Ahmet-Dedeler/bcpack-rs/blob/b1c3099a3402de8b76c9263a828791bd8bc0c930/README.md); license metadata: `NOASSERTION` | Texture codec research with reported synthetic tests; useful codec boundary, but exact tables/provenance/file licenses are unreviewed. Defer to #260. |
| `nullbrik/RageForge` | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| `woreksami/ttpf-studio` | [`a2fe110d`](https://github.com/woreksami/ttpf-studio/blob/a2fe110d8e7aeb8ed3be37c365c75078a408c089/README.md); license metadata: `MIT` | Texture/archive/video tooling. No original executable execution; no proprietary payload or replacement asset imported. |
| `walkthestars/GTA6-Asset-Info` | [`bad5bc21`](https://github.com/walkthestars/GTA6-Asset-Info/blob/bad5bc2180ade416eafdb50aa21e21eeec5adbda/README.md); license metadata: `GPL-3.0` | Asset-format notes for SixNG. Discovery only; derived samples/tables remain outside this review. |
| `Ahmet-Dedeler/awesome-gta6-reverse-engineering` | [`34a6e945`](https://github.com/Ahmet-Dedeler/awesome-gta6-reverse-engineering/blob/34a6e945b9698975ccd4e0c2e039034914dcc278/README.md); license metadata: `NOASSERTION` | Discovery index. Linked tools, code and asset claims have separate provenance and terms; no linked payloads were obtained. |
| `HolyRoses/gta6-explorer` | [`8e383a49`](https://github.com/HolyRoses/gta6-explorer/blob/8e383a49d07da5068fa30ebf208025e307018fcc/README.md); license metadata: `MIT` | Read-only package/asset inspection; not a guest runtime. No package bytes or format-derived corpus imported. |
| `meetox80/6stars` | [`480d9a4d`](https://github.com/meetox80/6stars/blob/480d9a4d00ed11560f7b4d989643723a0f5c451c/README.md); license metadata: `unresolved` | Swift/Metal world-data renderer. Native graphics demonstration is a different objective from executing original Xbox CPU/shader code. |
| `Ahmet-Dedeler/rage9-re` | [`30c6d215`](https://github.com/Ahmet-Dedeler/rage9-re/blob/30c6d2150ad9d3021802f509ff3ee232d466f499/README.md); license metadata: `NOASSERTION` | Animation/script asset-format tools, not executable compatibility. No asset samples or derived format tables imported. |
| `Albotterin/AWC_from_RPF9_Extractor` | [`decfde54`](https://github.com/Albotterin/AWC_from_RPF9_Extractor/blob/decfde54243d9a982550b08c7db21c34a20a1775/README.md); license metadata: `MIT` | Audio extraction tooling with upstream dependency/license scopes. Does not implement guest audio APIs; no extracted audio or code reuse. |
| `SentinelInteractive/0x52504639` | [`f1a383a7`](https://github.com/SentinelInteractive/0x52504639/blob/f1a383a72e90706b88d82f0ed1a674a0d7fcba58/README.md); license metadata: `MIT` | Format notes/catalogs, no execution evidence. Do not promote derived counts into console support. |
| `JoshRob297/rpf9-research` | [`144d3451`](https://github.com/JoshRob297/rpf9-research/blob/144d345191bd37de3f6bd5e85df3e8ae8815552e/README.md); license metadata: `GPL-3.0` | Container research/catalogs; protected-index/key questions are outside BetterXen scope. No derived indexes imported. |
| `Librett0/vice-lens` | [`8b8c8a84`](https://github.com/Librett0/vice-lens/blob/8b8c8a847ef926d8d6298ecfade142db19cfce5d/README.md); license metadata: `MIT` | Asset viewer/world renderer and media tooling; viewing a scene is not original-game execution. No asset/capture reuse. |
| `batatinhafcbatatinha-pixel/xseriesrecomp` | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| `sub-north/xbox-hle-lab` | [`8863028b`](https://github.com/sub-north/xbox-hle-lab/blob/8863028b486dedf660303fcd3c756b05bbbebf91/README.md); license metadata: `MIT` | Separate original probes, docs and XCL patches with different license scopes. Useful falsifiable-reproduction discipline; no patch reuse. |
| `kry-vex-gta/XBOX-Inspector` | [`0a3d067f`](https://github.com/kry-vex-gta/XBOX-Inspector/blob/0a3d067f7167a30b5e2517a4e781ec1ada3006d7/README.md); license metadata: `MIT` | Static inspection/recompilation prechecks. Metadata analysis is not guest execution; existing #211/#254 own comparable BetterXen work. |
| `nineforge0/forge9` | [`f40381ca`](https://github.com/nineforge0/forge9/blob/f40381cac62ade007a62e728eaa988fd9677bec4/README.md); license metadata: `MIT` | README advertises decryption and executable-derived key/table workflow. Excluded from implementation/input research; no such code or inputs accessed. |
| `TheoBIET/OpenVI` | [`fd415911`](https://github.com/TheoBIET/OpenVI/blob/fd4159118e750084451b9bad364169660daaa5e2/README.md); license metadata: `GPL-3.0` | Cross-platform asset explorer. macOS UI availability is not native emulation; no content or auto-update workflow executed. |


### Tracker candidates (30, including repeated XCL)

| Entry | Pinned public documentation | Classification and BetterXen decision |
| --- | --- | --- |
| `edsuuu/VI-BR-PORT` | [`7a30f506`](https://github.com/edsuuu/VI-BR-PORT/blob/7a30f5063dafeda1ceab84d251c22dc581d4dfc6/README.md); license metadata: `GPL-3.0` | VIctory-family Windows/NVIDIA memory tuning; wave32 assumptions must not be transferred to Apple GPU capability claims. |
| `xcl-dev/xcl` | [`f5c5bdf0`](https://github.com/xcl-dev/xcl/blob/f5c5bdf0e91035f8f1404f46f4f2f7aedc259e7f/README.md); license metadata: `GPL-2.0` | Windows x64 direct execution plus D3D12/DXIL; host services and pipeline behavior matter. GPL-2.0-only README; no reuse clearance. |
| `ZigZagBoi1/ScarlettPort` | [`48ba23a9`](https://github.com/ZigZagBoi1/ScarlettPort/blob/48ba23a91a83121f8d0625a633e88fa463d5a0ca/README.md); license metadata: `NOASSERTION` | Windows x64/D3D12 compatibility route; no native Darwin CPU/runtime established. |
| `Steinkoloss/port-of-the-americas` | [`0f3be36a`](https://github.com/Steinkoloss/port-of-the-americas/blob/0f3be36aaefe69c25f06e05222b88453f3557acd/README.md); license metadata: `unresolved` | Linux x64/Proton/vkd3d-proton shims; README names PE-Vulkan project shader conversion. No implementation reuse. |
| `Leucodermique/VIce` | [`8c7b24fd`](https://github.com/Leucodermique/VIce/blob/8c7b24fda90eb08fe64044726b7c880065a86ee0/README.md); license metadata: `GPL-3.0` | VIctory-based Windows distribution/frontend; consolidation does not independently validate combined changes. |
| `flipflapov/DSTN` | [`7066ae66`](https://github.com/flipflapov/DSTN/blob/7066ae669c7b0e20d0b05675826169cdc7442883/README.md); license metadata: `GPL-3.0` | PE-Vulkan project fork; tool portability on macOS does not establish native guest execution. No implementation reuse. |
| `daeken/XSXEmu` | [`2d937f83`](https://github.com/daeken/XSXEmu/blob/2d937f8360fbcdac368a0e363c24df322fa24db2/README.md); license metadata: `MIT` | macOS Rust host with Rosetta guest execution and separate native Metal shader fixtures; not a fully ARM64 guest CPU path. No game frame claimed. |
| `SixLauncher/VILauncher` | [`bf2f6ca9`](https://github.com/SixLauncher/VILauncher/blob/bf2f6ca9a6445aee6ebdff2b1cef87266f9cf1f4/README.md); license metadata: `GPL-3.0` | Linux launcher selecting VIctory/XCL under Wine/Proton. Not a new guest execution or Metal implementation. |
| `4Sakens/Project-Kalaga` | [`56b41fb4`](https://github.com/4Sakens/Project-Kalaga/blob/56b41fb4a68b89e5e356c1445699ba4ac6b8cc83/README.md); license metadata: `unresolved` | Windows consolidation of existing efforts; aggregated subsystems and local paths are not independent compatibility evidence. |
| `idvwork/rebox` | [`a833078e`](https://github.com/idvwork/rebox/blob/a833078e3366a70a74edef0cbfe01b80a81dd55c/README.md); license metadata: `GPL-2.0` | WineGDK/vkd3d-proton and AnyPS5 shader lineage; incompatible runtime and no emulator adaptation permitted. |
| `Yhozen/series-web` | [`d7512b29`](https://github.com/Yhozen/series-web/blob/d7512b299fe2836e08783665308cd8c4bb924345/README.md); license metadata: `GPL-2.0` | PISCO AOT x86-to-WebAssembly/runtime research; original browser workloads differ from native ARM64/Metal and Xbox compatibility. |
| `DesignWithTommy/Neonbridge` | [`954c00c5`](https://github.com/DesignWithTommy/Neonbridge/blob/954c00c506a8616e985e94e8ea6bcbfba7f80f48/README.md); license metadata: `GPL-3.0` | Windows VIctory-family launcher/runtime; frontend and startup improvements are not CPU translation for ARM64. |
| `moonrelax/basalt` | [`8af09135`](https://github.com/moonrelax/basalt/blob/8af091350e0d5cdf89f3736a756235a72b67bea9/README.md); license metadata: `MIT` | Original interpreter/cooperative scheduler research; GPU objects are fake and no rendering is claimed. Do not reuse stubs as acceptance. |
| `ka1qux/victory-ka1qux` | [`77b8a9eb`](https://github.com/ka1qux/victory-ka1qux/blob/77b8a9eb0d29521f34ffe6061530cc873090e844/README.md); license metadata: `GPL-3.0` | VIctory-family AMD Windows rendering fork. Vendor-specific counters and claimed fixes need independent original tests. |
| `ethan00013/keylime` | [`361c7e94`](https://github.com/ethan00013/keylime/blob/361c7e94ab9d29d88cb1e5bfaadc8c3fb909bebc/README.md); license metadata: `GPL-3.0` | Windows x64 runtime with Vulkan/SPIR-V; no native Mac route. Backend and console-specific claims need independent contracts. |
| `Joaco9020/mangrove` | [`d16326c6`](https://github.com/Joaco9020/mangrove/blob/d16326c652b6f82e7b59b390f63e37cab816acd3/README.md); license metadata: `GPL-3.0` | VIctory-family Windows fork tuned for lower-VRAM NVIDIA GPUs. Settings are not portable Metal correctness work. |
| `chvabalo-interactive/sludgemu` | [`29cf795e`](https://github.com/chvabalo-interactive/sludgemu/blob/29cf795e23bb4917b9a8acd6be95d38a0f3b99b1/README.md); license metadata: `unresolved` | Windows direct guest execution and D3D12/DXIL, early startup with logging stubs. Stubs are not behavior support. |
| `larpingvibecoder/sonoma` | [`835f386a`](https://github.com/larpingvibecoder/sonoma/blob/835f386a144eca0b903b8bec63c0a003d7e07071/README.md); license metadata: `Apache-2.0` | Windows Rust runtime/direct DXIL, early proof of concept; name does not indicate macOS support. |
| `7jycwjmbfn-eng/scenegate-vi` | [`d5908947`](https://github.com/7jycwjmbfn-eng/scenegate-vi/blob/d590894710631a3c6adc6cca526c44d3c0c2bd41/README.md); license metadata: `MIT` | Windows/Vulkan startup and scene work based on PE-Vulkan project; retain only documented failure categories, not code or title-derived inputs. |
| `TaynanGT/victory-windows-anterior-20261125` | Unavailable (404); no source recovered | Unavailable public endpoint; implementation, provenance and current execution status remain unknown. |
| `TaynanGT/victory-windows` | [`cdcff8fe`](https://github.com/TaynanGT/victory-windows/blob/cdcff8fe96182bade41343a10b985c028e9e8968/README.md); license metadata: `GPL-3.0` | VIctory-family Windows work on a non-main default branch; pin the actual branch, not its name. |
| `Archivist-Atlas/gta-vi-astral` | [`ff21332e`](https://github.com/Archivist-Atlas/gta-vi-astral/blob/ff21332e5ea2ea1c254a97c0b3036d36ff8e79ce/README.md); license metadata: `unresolved` | Go Windows runtime research citing VIctory/Leonida designs; no independent ARM64/Metal execution established. |
| `airpiglabs/crouton` | [`faad29e6`](https://github.com/airpiglabs/crouton/blob/faad29e6af4c45804cfa8a42901341a52d9c824c/README.md); license metadata: `GPL-2.0` | XCL/Bathyal lineage, Windows then Wine/Proton; GPL-2.0-only and incompatible host/runtime route. |
| `natedemoss/vice-pc` | [`2f98c053`](https://github.com/natedemoss/vice-pc/blob/2f98c05325237bc8ed8faf0e7b95bc0e6d22b9bb/README.md); license metadata: `MIT` | Windows loader proof of concept; README says nothing boots. No ARM64 or Metal evidence. |
| `JoseArranzSanchez/GTA6_PCPORT_INFO` | [`585b42a1`](https://github.com/JoseArranzSanchez/GTA6_PCPORT_INFO/blob/585b42a184bf1cdf1a5f8e76e0d2e7f8a9102857/README.md); license metadata: `NOASSERTION` | Secondary feasibility report, not implementation or primary legal authority. No legal conclusions adopted from it. |
| `DBWilliamEngineering/IraRationis` | [`67106e80`](https://github.com/DBWilliamEngineering/IraRationis/blob/67106e80ef34407f545b82bbd9b2c244f304ca3c/README.md); license metadata: `GPL-3.0` | C#/.NET AOT adaptation of ztrx; Windows x64 target remains. Review mixed license scopes if ever considering unrelated tooling. |
| `Guernica4u/GTA6-PC-Port-Archive` | [`7b69fa54`](https://github.com/Guernica4u/GTA6-PC-Port-Archive/blob/7b69fa5430ae19fd013b0ba19ffbd7badf3b93e8/README.md); license metadata: `unresolved` | Archive of other projects. Metadata/README screen only; do not use it to recover unavailable or prohibited implementation/material. |
| `idvwork/6stars-gta-vi-engine` | [`26a52ba2`](https://github.com/idvwork/6stars-gta-vi-engine/blob/26a52ba2ce5504c2c94a77c15838573538da5b70/README.md); license metadata: `unresolved` | Swift/Metal asset-world renderer, same stated approach as 6stars. Rendering data is distinct from executing the original guest; license/provenance unresolved. |
| `Ybucaille/nativ64` | [`3b826108`](https://github.com/Ybucaille/nativ64/blob/3b82610882bc55cb27f0ec563c126a226855a8ea/README.md); license metadata: `Apache-2.0` | Linux x86-64 Windows-API replacement library, not CPU translation. Its host-API inventory explains native-port work hidden by Windows pass-through. |
| `sleep10dev/XPORT` | [`2baf2a0f`](https://github.com/sleep10dev/XPORT/blob/2baf2a0f7c2af9f031158e082dfc4a6bb5b8266c/README.md); license metadata: `GPL-3.0` | Windows Rust loader/HLE/partial RDNA/D3D12 research, no demonstrated game compatibility. |


### Explicitly supplied Mac project and tracker

| Project | Pin | Decision |
| --- | --- | --- |
| `pragyaangaur/VIctory-macOS` | [`edba53dd`](https://github.com/pragyaangaur/VIctory-macOS/blob/edba53ddc2207e6c537679f4ce06f966b7a63a59/README.md); license metadata: `GPL-3.0` | Wine/CrossOver + Rosetta + D3DMetal guest route. Native platform unit tests are distinct from Windows-under-Wine GPU tests. |
| `4lwayz/portwatch-vi` | [`2cedf62a`](https://github.com/4lwayz/portwatch-vi/blob/2cedf62a16b63697f096763ecb99b4c10d4dbccc/README.md); license metadata: `unresolved` | Editorial discovery site and JSON data, not an emulator. Scores and live-stream summaries are not independent acceptance. |

## Verification performed and remaining limits

Read-only inventories ran for both the working-tree baseline and the extracted
current-main snapshot. Working-tree CPU/runtime counts were 491 forms/206
operations and 20 typed services; current main reports 498/213 and 47. Shader
reconciliation reported zero mismatches for both. These checks inspect catalogs;
they are not CPU instruction execution, GPU tests or independent acceptance.

Documentation checks also ran successfully on the working-tree candidate:
`python3 scripts/audit/status.py --check`, `scripts/audit/claims.sh` (22 claims),
`scripts/audit/dependencies.sh` (five graphics crates), Markdown local-file link
checks and `git diff --check`. These audit results do not verify runtime changes
present only on the separately inspected newer main snapshot.

Research metadata and ignored raw documentation are retained locally under
`artifacts/research-portwatch/`. GitHub API reads and issue/PR state checks
establish accessibility and observed state. Python HTTPS initially failed
certificate verification; authenticated `gh api` reads succeeded without
turning off certificate checks. No dependency was installed to work around it.

No Rust build, CPU execution suite, native GPU test, demo, fuzz, mutation or
benchmark ran for this documentation task. New acceptance must use the exact
candidate, actual native tool architecture and actual eligible hardware.
Independent reviewers have not certified this report's proposed implementations.
Nothing was committed, pushed, merged, released or installed by this review.
The GitHub research/follow-up issues were created at the user's request.
