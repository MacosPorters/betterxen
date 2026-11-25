# BetterXen development phases

This is BetterXen's working development plan. Each phase ends with a reproducible
application or engineering result. The plan is based on the current repository,
not on completing an inherited prompt or reaching a compatibility percentage.
No phase authorizes a release, publication, or commercial-game compatibility claim.

## Starting point

The source review used `19a905c30ea85166c05084da4fba67e356920912` on destination
`main`, with the local documentation work starting after the separate CODEOWNERS
commit `f02eb2678e76e91876be5706ce91dca25dcc3017`. The latter changes governance,
not runtime behavior. Open issues and PRs were inspected on November 24, 2026
(America/Chicago). Recheck their current heads and claims before taking work.

This task performed source inspection and read-only inventories. It did not
rerun the Rust, native GPU, application, fuzz, mutation, or performance suites.
Historical results below remain evidence for their recorded candidates only.

| Area | Present in the inspected source | Boundary for this plan |
| --- | --- | --- |
| Guest CPU | Generated x86-64 decoder, interpreter, ARM64 dynamic binary translator (DBT), integer and vector tests; inventory reports 491 declared forms and 206 operations | Declarations and lowering classifications do not establish independent acceptance or broad application compatibility. |
| Runtime | PE loading, import resolution, service handlers, process-exit tests and the compiled-C A1 fixture | The typed registry inventories only 20 services: 7 marked implemented and 13 partial. Many handwritten handlers are outside that inventory; 20 is not the total runtime service count. |
| Threads | Deterministic scheduler, guest thread contexts and guarded stacks, thread services and compiled-C thread tests | Guest threads execute serially on one host thread. Blocking synchronization, sleep behavior and thread lifecycle callbacks still have documented gaps. |
| Graphics | gfx1030 shader decoder/compiler/reference, native Metal execution paths, texture and draw tests | Synthetic desktop instruction fixtures do not establish Xbox behavior. Acceptance is tracked separately from local readback. |
| Native app | Swift app, Rust C interface, controller input and audio/virtual-filesystem fixtures | Existing fixtures do not establish one integrated threaded C++ 3D application. |
| Quality tools | CPU/runtime inventories, shader coverage reconciliation, task queue, PR checks and acceptance records | Some inventories and ledgers contain stale limitation text. A successful tool invocation does not prove every catalog entry is current. |

Sources: [CPU registry](cpu/instruction-registry.md),
[runtime registry](runtime/registry.md), [guest threads](runtime/threads.md),
[interfaces](interfaces.md), [shader coverage](shaders/semantic-coverage.md),
[Gate A](testing/gate-a.md), and [QA tooling](testing/infrastructure/README.md).
The read-only shader reconciliation reported zero mismatches during this task;
that result is inventory consistency, not new GPU verification.

## How previous phase names fit

Keep historical evidence and attribution intact. These records remain useful,
but their phase numbers are not the sequence for new BetterXen work.

| Existing record or name | Meaning | How to use it now |
| --- | --- | --- |
| [Phase 01 checkpoints and assurance](testing/phase01-assurance.md), [risk register](risk-register.md), [review log](testing/review-log.md) and [mutation record](testing/mutation.md) | Earlier bounded shader research and candidate-specific gates | Consult the original gate definitions and unresolved findings; do not relabel them as current acceptance. |
| [Phase 02 GPU/guest baseline](testing/phase-02-baseline.md) and [CPU feasibility](research/phase02-cpu-feasibility.md), [ecosystem](research/phase02-ecosystem.md), [collaboration](research/phase02-collaboration.md) research | Early graphics expansion and CPU feasibility work | Reuse documented public sources and evidence limits, checking them against current source. |
| [CPU/runtime campaign ledger](coordination/cpu-campaign-ledger.md) | Implementation batches and local evidence | Its older limitations are not a current feature inventory: scheduler, atomic and string work has since landed. |
| [Infrastructure Phase II/III tooling](testing/infrastructure/README.md) and [compatibility ledger](testing/infrastructure/phase3-compat-ledger.md) | A separate QA campaign | Tool code now includes compatibility-gap analysis and verification profiles beyond the ledger's early checkpoint. Coordinate with existing issue #219 rather than starting another QA framework. |
| [Gate A](testing/gate-a.md) | Compiled C execution, split into A1 and hosted-C A2 | A1 has a main-branch test. A2 is under review in PR #283; the document's “not started” description predates that PR. Neither is independently accepted by this plan. |
| Native app numbered gates in `crates/ffi` and `app` | Bounded session, graphics, audio and filesystem fixtures | Inspect their tests before proposing integration. Gate numbers are separate from shader acceptance gates. |
| README X1–X25 milestones | Broad capability map | Retain as a capability reference; use the phases below for the next work order. |
| Supplied Discord III, III-B, IV, V and VI excerpts | Broad runtime, graphics, application and performance objectives | Only excerpts were supplied. Their omitted requirements and completion status are unknown; they are not execution instructions. |

Bare issue numbers in historical documents may refer to the source repository.
All issue and PR links in the work sections below refer to **MacosPorters/betterxen**.
Use [migration records](migration.md) to resolve historical references.

## Phase 1 — Establish a trustworthy working baseline

**Outcome:** contributors can tell what exists on main, what is pending review,
and which checks actually ran on the candidate they are using.

1. Reconcile the current source with the CPU campaign ledger, Gate A document,
   runtime inventory scope, and QA compatibility ledger. In particular, the QA
   CPU inventory's intentional-rejection catalog still lists strings and
   atomics/fences despite source and tests for supported subsets. Correct the
   catalog through its existing owner; do not remove explicit unsupported forms.
2. Preserve all imported attribution. Repair destination links in migrated PRs
   without recreating sign-offs or treating archived reviews as current approval.
3. Reproduce the applicable existing CPU, generator and repository checks for a
   pinned candidate. Run GPU and app checks separately on an eligible native host.
   Record errors, unavailable tools and skipped tests separately.
4. Review conflicting PR branches with their authors. Changing descriptions
   cannot resolve source conflicts or provide missing ARM64 build checks.
5. Finish BetterXen's active branding where needed. Preserve copyright notices,
   licenses and accurate historical records. Package names, C symbols, guest ABI
   names and environment variables require a compatibility migration, not a
   blanket search-and-replace. Do not rewrite authorship to imply new ownership.

Existing work: [migration checks #286](https://github.com/MacosPorters/betterxen/issues/286),
[QA scope #219](https://github.com/MacosPorters/betterxen/issues/219),
[app CI #248](https://github.com/MacosPorters/betterxen/issues/248).

**Exit criteria:** a candidate SHA, a current capability/blocker inventory,
links to retained logs for each executed gate, explicit pending reviews, and
usable PR descriptions. Independent acceptance remains a separate decision.
No release is required.

## Phase 2 — Finish a reproducible hosted C application

**Outcome:** an original hosted C program starts, uses the runtime, reports
results and exits through the guest path with checked cleanup.

Review and integrate existing work rather than creating a competing runtime:

| Prerequisite | Existing destination work |
| --- | --- |
| CRT startup calls | [issue #237](https://github.com/MacosPorters/betterxen/issues/237), [PR #273](https://github.com/MacosPorters/betterxen/pull/273) |
| Arguments and environment | [issue #242](https://github.com/MacosPorters/betterxen/issues/242), [PR #277](https://github.com/MacosPorters/betterxen/pull/277) |
| Normal termination and exit handlers | [issue #243](https://github.com/MacosPorters/betterxen/issues/243), [PR #280](https://github.com/MacosPorters/betterxen/pull/280) |
| Formatted standard output | [issue #245](https://github.com/MacosPorters/betterxen/issues/245), [PR #281](https://github.com/MacosPorters/betterxen/pull/281) |
| File streams | [issue #246](https://github.com/MacosPorters/betterxen/issues/246), [PR #282](https://github.com/MacosPorters/betterxen/pull/282) |
| Integrated hosted-C fixture | [issue #262](https://github.com/MacosPorters/betterxen/issues/262), [PR #283](https://github.com/MacosPorters/betterxen/pull/283) |

Check dependency order and stacked branches against their actual heads. Audit
the compiler and linked CRT's per-file licensing and native ARM64 build tools.
An x86-64 guest fixture is permitted; its compiler must run natively on ARM64.

**Exit criteria:** reproducible original fixture build; checked stdout/stderr,
arguments, file contents, exit status, callback order and cleanup; unresolved
imports and exhausted budgets fail explicitly. Existing A1 behavior still passes.
Independent reviewers derive expected results from public specifications and the
fixture contract. No general C++ or Xbox startup claim follows from this result.

## Phase 3 — Make threaded application behavior coherent

**Outcome:** an original worker-based application can block and resume without
stalling unrelated guest work or leaking per-thread state.

Extend the existing scheduler under [issue #228](https://github.com/MacosPorters/betterxen/issues/228).
Prioritize a small fixture that exposes a documented gap: guest sleep currently
delays all guest threads, and critical-section/SRW/condition-variable waits are
not fully connected to scheduling. Claim one gap and its paths before editing.

Then add the thread lifecycle behavior required by that workload, including
static thread-local storage (TLS), library attach/detach and fiber-local storage
(FLS) cleanup where supported. Keep unsupported callbacks explicit until their
semantics and safety are implemented. Preserve Win64 nonvolatile register checks
and coordinate with [issue #193 / PR #268](https://github.com/MacosPorters/betterxen/pull/268).

**Exit criteria:** deterministic results at several scheduling quanta; another
worker progresses while one waits; timeouts, cancellation, abandoned objects,
thread-local errors, stack bounds and cleanup have negative controls. Repeat
runs reclaim thread resources. Serial guest scheduling remains the execution
model unless a separately reviewed memory-ordering design replaces it.

## Phase 4 — Connect resource updates to translated rendering

**Outcome:** one original guest workload updates a texture and geometry, submits
translated rendering, waits for completion and produces an independently checked
frame through the existing native Metal backend.

Start with a bounded scene and its concrete missing features. Do not make all
RDNA2 instructions, mesh shaders or ray tracing prerequisites for a first scene.
Reuse the semantic registry and distinguish declared, decoded, lowered,
reference-tested, locally GPU-tested and independently accepted forms.

Coordinate [RDNA2 expansion #205](https://github.com/MacosPorters/betterxen/issues/205),
[mip footprints #241 / PR #278](https://github.com/MacosPorters/betterxen/pull/278),
[graphics fences #224](https://github.com/MacosPorters/betterxen/issues/224),
[shared resource lifetime #148](https://github.com/MacosPorters/betterxen/issues/148),
and [texture-update workload #147](https://github.com/MacosPorters/betterxen/issues/147).
Wave divergence, cross-lane operations and additional formats get separate
fixture contracts and claims when this workload needs them.

**Exit criteria:** actual Metal execution and frame readback against an
independently derived expectation; update/submission/retirement order is checked;
invalid descriptors, stale handles and unsupported shader forms fail explicitly.
Record host capability detection, hardware, validation settings and evidence
class. Test counts and tolerances retain their existing definitions. Results
remain synthetic desktop-gfx1030 evidence, never Xbox hardware verification.

## Phase 5 — Run one integrated interactive application reliably

**Outcome:** a small, original application joins guest execution, rendering,
input, bounded audio, filesystem access and lifecycle behavior in one session.

Start from [multi-module startup #256](https://github.com/MacosPorters/betterxen/issues/256)
and the existing app/FFI fixtures. First prove a compiled workload can reach a
checked frame. Add C++ language/runtime features only when an independently
authored failing fixture demonstrates the need. Audit any linked runtime first.
Do not treat the Swift audio/input demonstrations as guest integration evidence.

Coordinate [audio #227](https://github.com/MacosPorters/betterxen/issues/227),
[session errors #250](https://github.com/MacosPorters/betterxen/issues/250),
[report schema #254](https://github.com/MacosPorters/betterxen/issues/254),
[worker diagnostics #255](https://github.com/MacosPorters/betterxen/issues/255),
and [application quick start #247](https://github.com/MacosPorters/betterxen/issues/247).
Execution-admission changes remain owned by [issue #212](https://github.com/MacosPorters/betterxen/issues/212).

**Exit criteria:** scripted input and deterministic checkpoints; start, pause,
resume, stop and restart behavior; checked audio sample data before device
playback; bounded memory and handle use; sandbox escape attempts rejected;
useful failure reports and a repeatable soak protocol. Specify soak duration,
resource limits and acceptance conditions before measuring. A fixture worker
passing does not certify arbitrary files safe or commercial titles playable.

## Phase 6 — Improve measured bottlenecks

**Outcome:** faster or lower-resource execution of workloads whose correctness
is already checked, with measurements that another contributor can reproduce.

Performance analysis can begin earlier; changes must preserve the relevant
phase's correctness. Select bottlenecks from actual measurements rather than
assuming every shared-semantics call, allocation or cache miss needs replacement.
Potential work includes CPU helper lowering, bounded translation-cache policy,
Metal startup, texture transfers and frame-loop overhead.

Reuse [startup measurements #171 / PR #266](https://github.com/MacosPorters/betterxen/pull/266),
[TLS optimization #107 / PR #284](https://github.com/MacosPorters/betterxen/pull/284),
[DBT comparison #110 / PR #272](https://github.com/MacosPorters/betterxen/pull/272),
and [conditional cache prewarming #258](https://github.com/MacosPorters/betterxen/issues/258).

**Exit criteria:** correctness gates pass on the changed candidate; baseline and
candidate use equivalent inputs, toolchains, settings and hardware; idle-machine
samples include variance and separate cold/warm startup. Report regressions as
well as gains. Run validation-off benchmarks separately from validation-on GPU
correctness. Capability differences across Apple Silicon generations remain
unknown until measured; runtime detection controls optional paths.

## Taking the next task

Begin with Phase 1's evidence reconciliation and review Phase 2's existing PRs.
The phase-plan issue does not claim their implementation files. Read every
comment on a target issue and inspect linked PRs; never take a live claim.
Issue #219's QA paths and subsystem owners remain independent of this plan.

Each implementation task needs a bounded outcome, permitted files, dependencies,
public semantic sources, a failing fixture or other meaningful acceptance check,
and the required independent reviewers. Use original code and expectations;
never copy another emulator to satisfy a phase. Do not require a fixed number
of agents: use available resources only within the repository's LOW-reasoning
and independent-review rules.

Useful existing commands for the candidate record:

```sh
PYTHONPATH=scripts python3 -m qa cpu-inventory --compact
PYTHONPATH=scripts python3 -m qa runtime-inventory --compact
PYTHONPATH=scripts python3 -m qa rdna2 reconcile --compact
python3 scripts/audit/status.py --check
scripts/audit/claims.sh
scripts/audit/dependencies.sh
```

For implementation, run the applicable checkout commands in `AGENTS.md` and the
subsystem acceptance protocol. Keep logs under ignored `artifacts/`. Use at most
four Cargo jobs across two CPU-heavy jobs, one GPU job and one fuzz or mutation
campaign at a time. New verification must name the exact candidate; skipped
tests never count as GPU evidence. Passing a phase does not authorize a release.
