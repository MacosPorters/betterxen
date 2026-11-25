# BetterXen development phases

BetterXen's objective is to execute supported Xbox Series software through an
original native ARM64 runtime and direct Metal on macOS. Commercial game
execution, including GTA VI, is currently unsupported. The demonstrated results
are bounded, independently authored programs and shaders.

This is the sole active phase sequence. It replaces the previous phase plan and
the README's X1–X25 roadmap. Historical fixture names and acceptance records
retain their meaning as evidence for their original candidates; they are not a
second development schedule.

## Starting position

The reviewed main revision is
[`b2654e93a42e74e29570183da1f31bb8cdc6f721`](https://github.com/MacosPorters/betterxen/tree/b2654e93a42e74e29570183da1f31bb8cdc6f721),
checked on November 24, 2026 (America/Chicago). Research began from a working
tree at `f02eb2678e76e91876be5706ce91dca25dcc3017` and inspected newer main in a
separate snapshot. This documentation change is based on the reviewed main
revision; it changes no runtime implementation.

On inspected main, the source contains an x86-64 interpreter, an ARM64 dynamic
binary translator (DBT), PE32+ loading, a partial runtime, a deterministic guest
scheduler, translated graphics, texture/sampler bindings and a native Swift app.
The hosted C fixtures A1 and A2 are implemented and reported locally passing;
both still await independent acceptance. These pieces have not established a
complete commercial game or one integrated threaded 3D application.

The [public-project review](research/portwatch-native-macos.md) explains the
external comparisons and current-source limits. Windows projects often execute
x86-64 instructions directly and use Windows/D3D12 services. The inspected Mac
guest-execution projects use Rosetta; native asset renderers demonstrate a
different workload. Their progress is a research lead, not BetterXen evidence.
No competitor emulator implementation may be copied or adapted.

No phase is declared complete by this planning task. It ran documentation and
inventory checks, not new CPU/GPU execution, fuzzing, mutation or benchmarks.

## Delivery sequence

| Phase | Demonstrable result | Principal dependency |
| --- | --- | --- |
| 1 | A reproducible compiled guest boots and exits on native ARM64 | Existing A1/A2 and native tool evidence |
| 2 | One modular guest program runs workers and cleans up correctly | Module startup, scheduling and lifecycle contracts |
| 3 | Guest code controls a translated textured Metal scene | Validated guest graphics calls, bindings and completion |
| 4 | An original interactive application combines execution, graphics, input, audio and saves | The supported paths from phases 2 and 3 |
| 5 | A varied original workload corpus exposes and closes specific compatibility gaps | A repeatable integrated application and diagnostics |
| 6 | Sustained execution is measured and verified across native Apple Silicon hosts | Checked workloads, resource bounds and actual host reports |
| 7 | A separately admitted Xbox compatibility experiment has documented contracts and reproducible results | Public console evidence and all behavior required by that input |

Each result has its own admission profile. A simple scene need not wait for all
AVX2 instructions, mesh shaders or ray tracing. A workload needing an unsupported
feature stops explicitly. Research, diagnostics, CPU expansion and performance
measurement can proceed alongside these phases on independently claimed paths.
A phase's number does not grant exclusive ownership of a subsystem.

## Phase 1 — Boot a reproducible compiled guest

**Deliverable:** a contributor can build an original guest from source, start it
through BetterXen's supported test entry point and reproduce its output and exit
without an Intel host executable or Windows compatibility runtime.

Use the existing [Gate A fixtures](testing/gate-a.md). On inspected main,
[PR #283](https://github.com/MacosPorters/betterxen/pull/283) already supplies the
hosted C program. Reproduce and review that implementation; do not create a
competing CRT startup or mark it unimplemented because this checkout is older.

The candidate record identifies the exact source revision, local modifications,
native compiler/linker, linked support-code licenses, execution path and limits.
Check host executables/dependencies with `file`, `lipo -archs` and `otool -L`.
Guest x86-64 bytes are permitted data; native host tools must support ARM64.
Use `aarch64-apple-darwin` for BetterXen.

**Completion checks:**

- A1/A2 reproduce the specified stdout/stderr, files, callback order, exit status
  and cleanup on the named candidate. Missing compilers fail visibly.
- Existing wrong-input, unresolved-import and instruction-budget controls still
  fail for their specified reasons. A trapped stub is not implemented behavior.
- The admission profile states which CPU forms, imports and startup features it
  supports. The test entry point is distinguished from CLI/app execution.
- Independent reviewers accept the semantic expectations and the scope of the
  native evidence. A local pass alone does not close this phase.

**Contributor work:** [CPU coverage #154](https://github.com/MacosPorters/betterxen/issues/154),
[native reports #303](https://github.com/MacosPorters/betterxen/issues/303), and
[existing compatibility report #254](https://github.com/MacosPorters/betterxen/issues/254).
Use existing audit/test tooling and coordinate schema changes with its owners.

## Phase 2 — Run a modular program with guest workers

**Deliverable:** an original executable and companion libraries initialize,
start workers, exchange results, block/resume and exit with checked teardown.

Begin with [the EXE/two-DLL startup contract #301](https://github.com/MacosPorters/betterxen/issues/301)
and [worker lifecycle design #305](https://github.com/MacosPorters/betterxen/issues/305).
The fixture records dependency initialization, per-module static thread-local
storage (TLS), worker-visible values and supported detach/destructor calls.
Expected order must be derived before implementing the loader or callbacks.
Built-in service-module lookup is not general guest DLL loading.

Extend the existing serial scheduler under
[#228](https://github.com/MacosPorters/betterxen/issues/228). Its documented
`Sleep` behavior can stall every worker; [#304](https://github.com/MacosPorters/betterxen/issues/304)
owns the narrow yielding fix after parent/runtime ownership coordination.
Handle critical sections, condition variables and other waits as separately
specified gaps, not incidental additions to that fix.

Broader CPU behavior is driven by the admitted fixture. Start
[AVX/YMM design #299](https://github.com/MacosPorters/betterxen/issues/299) and
[unwind design #300](https://github.com/MacosPorters/betterxen/issues/300) in
parallel, but do not require their entire future scope for a simpler program.
A workload using those features must wait for real support. CPU feature bits
must not advertise unsupported behavior. Stack walking, structured exception
handling (SEH) and C++ exceptions are separate capabilities.

**Completion checks:**

- The source-built module graph has independently derived results and
  initialization/teardown records, with explicit forwarder/cycle/search policy.
- One worker progresses while another waits. Results hold across specified
  scheduling quanta; the injected clock and timeout units remain consistent.
- Static TLS, dynamic TLS, fiber-local storage (FLS), errors, errno and CPU state
  retain their specified per-thread isolation. Dropping a value does not count
  as executing its registered guest callback.
- Missing exports, failed attach, invalid callback targets, stack faults,
  timeouts, deadlock and exhausted budgets have precise, bounded outcomes.
- Repeated runs reclaim owned modules, handles, stacks and callback state.
  Unsupported delayed imports or lifecycle paths are rejected explicitly.

**Contributor work:** reuse [package census #211](https://github.com/MacosPorters/betterxen/issues/211)
and the existing scheduler. Runtime PRs #263/#290 overlap; their authors and
maintainers must resolve ownership before shared production edits. Parallel
host-thread execution remains outside the serial scheduler's ordering claim.

## Phase 3 — Render a scene controlled by guest code

**Deliverable:** a compiled original guest creates resources, uploads a texture
and geometry, submits translated rendering, waits for completion and checks the
resulting frame through its guest-visible interface.

Use [startup-to-frame integration #256](https://github.com/MacosPorters/betterxen/issues/256)
as the integration task. A Rust test calling a host graphics method does not
alone prove the compiled guest can call it. Specify the supported service/call
boundary and check its Win64 argument/return and nonvolatile-state behavior.
Guest addresses must resolve to checked ranges and opaque handles, never host
pointers.

Existing translated `image_sample_lz`, guest descriptor-table bindings, texture
upload paths, blend/index/depth tests and packed compressed-texture footprints
are starting components. [Binding/entry design #302](https://github.com/MacosPorters/betterxen/issues/302)
specifies expansion beyond the bounded texture/sampler contract. Use an original
scene with distinguishable textures and resource updates so wrong bindings or
stale data change checked pixels.

**Completion checks:**

- Actual native Metal execution produces frame readback matching expectations
  derived independently of the decoder, compiler and production renderer.
- Changing one guest texture, transform or binding changes the expected frame;
  a skipped draw or substituted shader cannot pass as correct rendering.
- Update, submit, completion, readback and retirement order are checked.
  Resources remain valid for in-flight work and stale handles are rejected.
- Bad descriptors, overflowing ranges, unsupported shader forms and invalid
  resource states fail before unsupported work reaches Metal.
- The report records device capability queries, validation settings, candidate,
  commands and hardware. Synthetic gfx1030 success is not Xbox GPU verification.

**Contributor work:** [resource-update workload #147](https://github.com/MacosPorters/betterxen/issues/147),
[shared lifetime #148](https://github.com/MacosPorters/betterxen/issues/148),
[fences #224](https://github.com/MacosPorters/betterxen/issues/224), and
[format inventory #260](https://github.com/MacosPorters/betterxen/issues/260).
Respect the active format claim; do not duplicate its inventory.

## Phase 4 — Run an original interactive application

**Deliverable:** one small original game-like application runs through the guest
CPU/runtime and native app with translated graphics, scripted input, bounded
audio and a save/load cycle in a persistent session.

Compose the admitted paths from phases 2 and 3. Define the application source,
input replay, frame checkpoints, sample-data checks and save format before
implementation. Start with the CPU/shader subset already justified by the scene.
Add C++ or additional runtime functionality only when an original failing
fixture demonstrates the requirement and linked runtime terms are reviewed.

Wire the application through the existing Swift/Rust interface and worker
lifecycle. Existing native audio, controller and filesystem demonstrations are
components, not proof that guest code reaches each service. This phase must
exercise those guest connections in one session.

**Completion checks:**

- A deterministic replay reaches independently specified state/frame checkpoints.
- Input reaches guest logic; expected audio samples are checked before device
  playback; save/load preserves specified state and reports I/O failures.
- Start, pause, resume, stop, restart and worker failure have defined outcomes.
  Commands do not report success after a worker has failed.
- Resource/handle counts and cleanup remain bounded across repeated sessions;
  invalid guest paths and resource requests fail according to the interface.
- The native app/CLI provide a reproducible quick start and useful stage-specific
  failure reports. This fixture still establishes no commercial compatibility.

**Contributor work:** [audio #227](https://github.com/MacosPorters/betterxen/issues/227),
[session errors #250](https://github.com/MacosPorters/betterxen/issues/250),
[worker diagnostics #255](https://github.com/MacosPorters/betterxen/issues/255),
[app CI #248](https://github.com/MacosPorters/betterxen/issues/248), and
[quick start #247](https://github.com/MacosPorters/betterxen/issues/247).
Execution-admission policy remains with
[#212](https://github.com/MacosPorters/betterxen/issues/212).

## Phase 5 — Expand compatibility through a varied workload corpus

**Deliverable:** a versioned set of original source-built workloads exercises
meaningfully different CPU, runtime and graphics behavior through the integrated
path, with precise accepted/rejected profiles and reproducible regressions.

Choose additions from actual failures. Examples include another compiler or
runtime profile, vector-heavy arithmetic, runtime-dependent shader branches,
bounded loops, multiple resource views and streaming texture updates. Each
addition must demonstrate a gap not already owned by an existing task.

Use [CPU design #299](https://github.com/MacosPorters/betterxen/issues/299),
[unwind design #300](https://github.com/MacosPorters/betterxen/issues/300),
[runtime uniform branches #179](https://github.com/MacosPorters/betterxen/issues/179),
[bounded shader loops #180](https://github.com/MacosPorters/betterxen/issues/180),
and [RDNA2 expansion #205](https://github.com/MacosPorters/betterxen/issues/205).
Wave divergence, cross-lane operations, local data share (LDS), barriers, mesh
stages and ray tracing require their own public contracts, fixtures and native
capability checks when a selected workload needs them.

**Completion checks:**

- Each workload identifies its source/build recipe, required forms/services,
  original expected outputs, limits and exact independently accepted candidate.
- Coverage separates declared, decoded, lowered, CPU-tested, locally GPU-tested
  and independently accepted behavior. Compilation counts are not compatibility.
- Unsupported behavior retains explicit diagnostic controls; expected values do
  not come from the same implementation being tested.
- Targeted mutation/fuzz/regression checks demonstrate sensitivity to the changed
  behavior without altering tolerances, gate definitions or mutation counting.
- Earlier integrated workloads still pass. Failures become bounded tasks with
  proposed files and dependencies, not a generic rewrite of a subsystem.

**Contributor work:** [original corpus #150](https://github.com/MacosPorters/betterxen/issues/150),
[shader acceptance #225](https://github.com/MacosPorters/betterxen/issues/225),
CPU coverage #154 and the existing QA owner #219. Reuse their tools and acceptance
rules; do not create another competing status or test framework.

## Phase 6 — Sustain execution across Apple Silicon

**Deliverable:** admitted workloads run for a specified duration under bounded
memory/resource use, and performance/correctness reports are reproducible on
the actual native hosts contributors test.

Specify soak duration, replay, resource limits and failure conditions before the
run. Measure guest execution, translation, service calls, transfers, Metal
startup, submission and readback to identify the actual bottleneck. Record
cold/warm behavior, variance and regressions as well as improvements.

Use the existing native audit, GPU tests and benchmarks. Reuse merged startup
measurements and DBT comparison tooling; evaluate prewarming/persistence only
under [#258](https://github.com/MacosPorters/betterxen/issues/258) when measurements
justify it. Guest code-cache lifetime and same-block executable writes remain
explicit CPU correctness risks under #95 and #291.

**Completion checks:**

- The agreed soak/restart protocol passes on the pinned candidate with recorded
  resource accounting and no hidden fallback or silent loss of work.
- Baseline/candidate measurements use equivalent inputs, toolchains, settings
  and hardware. Validation-on correctness and validation-off performance are
  separate runs; benchmarks use an idle host as required by the protocol.
- [Native reports #303](https://github.com/MacosPorters/betterxen/issues/303)
  distinguish host binary architecture, documented capabilities, actual execution,
  failure, unavailable tools and skipped tests. Skips are never GPU passes.
- Runtime capability detection selects optional paths. M1–M5/future-device cells
  stay unknown until actual reports exist; one machine cannot verify them all.
- Contributor preview preparation follows app/native gates. Signing, notarization,
  redistribution review and publication require their own evidence and explicit
  release authorization; this phase does not authorize uploads.

**Contributor work:** [soak #149](https://github.com/MacosPorters/betterxen/issues/149),
[memory pressure #204](https://github.com/MacosPorters/betterxen/issues/204),
[dependency evidence #158](https://github.com/MacosPorters/betterxen/issues/158),
and [preview preparation #259](https://github.com/MacosPorters/betterxen/issues/259).

## Phase 7 — Attempt documented Xbox compatibility

**Deliverable:** a separately reviewed compatibility experiment with admissible
inputs demonstrates exactly the supported startup/execution/rendering stages,
or records a specific unresolved contract that prevents the attempt.

Begin [public console-contract research #306](https://github.com/MacosPorters/betterxen/issues/306)
now, alongside the earlier engineering phases. Maintain evidence for launch and
CPU entry state, executable admission, service ABI, shader entry/bindings,
resource and command layouts, synchronization and memory aliases. Public PE,
AMD desktop ISA and D3D12 documents establish their documented contracts; they
do not automatically establish Xbox behavior. Unknown console details remain
unknown rather than becoming guessed implementations.

An actual execution attempt depends on demonstrated technical support for its
input, project-permitted authorized/unprotected material, explicit admission
and independent review. Do not obtain or use leaked executables, restricted
SDKs, confidential headers, proprietary shader collections, firmware or keys;
do not implement protection bypasses. A torrent filename or container does not
establish a supported executable, launch environment or compatibility result.
User files stay outside the repository. File validation concerns technical
compatibility and safety; it must not demand receipts, accounts or ownership
provenance uploads.

**Completion checks:**

- Required contracts have precise public sources and repository evidence classes,
  or are visible blockers. Restricted/unavailable evidence is not bypassed.
- The admitted input profile, candidate, runtime capabilities and expected
  outcomes are reviewed before execution and precisely bounded.
- Actual results separately identify inspection, loading, entry, menu/frame,
  world simulation, interaction and sustained play. Reaching one does not
  certify the next; crashes and missing behavior remain in the report.
- R1 audits evidence/provenance and R2 independently derives semantic expectations.
  BetterXen has no Xbox hardware access and must never claim such verification.
- No game title is called supported or playable without a demonstrated repeatable
  result. No date, compatibility percentage or release follows from this plan.

This phase can remain blocked while earlier original workloads continue to
improve. There is currently no demonstrated GTA VI execution in BetterXen.

## How contributors start

The [research tracker #298](https://github.com/MacosPorters/betterxen/issues/298)
links the eight new bounded tasks #299–#306 and the existing work they depend on.
The phase plan claims no implementation files. Research/design tasks can begin
on their documented paths; production changes must coordinate subsystem owners
and active claims. Read all comments, including imported archived claims, and
inspect linked open PRs before claiming one issue in a plain reply.

Every task needs an outcome, proposed files, dependencies, public semantic
sources, meaningful positive/negative checks and independent reviewers. Update
the changelog for meaningful changes. Record candidate-specific results and raw
logs under ignored `artifacts/`; distinguish implementation, local verification
and independent acceptance. Preserve historical evidence and attribution.

Run the applicable commands from `AGENTS.md`. Across simultaneous work use at
most four Cargo jobs across two CPU-heavy jobs, one GPU job and one fuzz or
mutation campaign. Never count unrun or skipped checks as passes. No commit,
push, merge or release is authorized by a phase or an issue claim.
