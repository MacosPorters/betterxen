# Risk register

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

| # | Risk | Cause | Impact | Detection | Mitigation |
| --- | --- | --- | --- | --- | --- |
| R1 | ABI incompatibility | Extending an existing struct or changing a signature | Shim load or memory corruption | Export verifier, `static_assert` on layouts | Additive functions and new structs; `[ABI]` PR first |
| R2 | Pointer instability | Migrating or remapping an exposed allocation | Guest memory corruption | Fixture holds pointers across reconfigure and device changes | Never move an exposed pointer; refuse instead |
| R3 | Incorrect alignment | Assuming `Map` pointers are 64 KiB aligned | Faults or wrong data | Runtime check on the returned pointer | Check, fall back before exposure |
| R4 | Double charging | Charging a pool and its slices, or heap and placed resource | Spurious out-of-memory | Ledger fixtures with known totals | Charge backing once |
| R5 | Under-accounting | A creation path not covered (`CreateHeap1`, committed resources, swap chain) | Cap silently exceeded | Fixture per path; footprint comparison | Enumerate paths; log uncounted ones |
| R6 | Premature resource reuse | Recycling before every queue finished | Silent corruption | Delayed-fence fixture, guard data | Recycle only BetterXen-submitted use |
| R7 | Early charge release | Crediting on wrapper release | Over-commit | Release-heap-keep-resource fixture | Credit on confirmed destruction |
| R8 | Fence failures | Failed signal, timeout, device removal | Use after free | Injected failures | Quarantine, never recycle |
| R9 | Deadlocks | Locks held across host calls or in `Release`/`DllMain`; message-pump re-entry | Hang | Timeouts under supervision, stress | Minimal critical sections; no host calls under the authority lock |
| R10 | Driver-retained memory | D3DMetal keeps freed allocations charged | Host pressure although the ledger is low | Footprint sampling | Report separately; do not assume reclaim |
| R11 | Allocation overhead | Wrapping, logging, reconciliation walks | Slower allocation | Churn benchmark | Measure; make logging sampled |
| R12 | Texture regressions | Routing textures through buffer logic | Wrong images | Texture readback fixtures | Textures stay on their own path |
| R13 | DirectStorage incompatibility | Wrapped resources reaching host calls, changed states | Failed or corrupt loads | `test_xcrdapi` on each change | Unwrap audit; keep staged path |
| R14 | Unintended copying | A "zero-copy" path that copies internally | No benefit, or regression | Timing and transfer-byte measurements | Never claim physical sharing; measure |
| R15 | Hardware-specific behaviour | Results from M4/M5 only | Failures elsewhere | Capability logging, wider testing | Runtime capability checks; label untested |
| R16 | Runtime-version behaviour | CrossOver or D3DMetal changes destruction or import behaviour | Ledger or lifetime errors | Re-run probes per version | Record versions; keep the default path |
| R17 | Dependency licensing | Vendoring an allocator without records | Non-compliance | Registry and notice review | Pin, record, preserve notices before use |
| R18 | Migration conflicts | Parallel edits to `proxy.cpp` and generated files | Rework | Claim and PR checks | Provider code in new files; one owner per stage |
| R19 | Vtable patching | #486 rewrites two proxy vtable slots at run time | Races; surprising dispatch | Concurrency review | Use ordinary overrides |
| R20 | Accounting presented as improvement | Lower ledger numbers without real savings | Wrong decisions | Compare footprint and transfers | Report all layers together |
| R21 | Wrong `XMemVirtualAlloc` parameter list | Four declared, five at public call sites | Wrong protection; graphics bits lost | Maintainer review; synthetic fixture once the contract is agreed | Resolve before relying on attributes |
| R22 | Enforcement causes new failures | A cap now applies to allocations that never failed | Titles stop where they ran before | Would-reject log in Stage 0a | Observe first; switch |
| R23 | Release callback timing or re-entrancy | Thread and moment unspecified | Deadlock, or charge held long enough to refuse a reallocation | Stress with immediate free and reallocate | Lock-free release path |
| R24 | Allocations outside the proxy | Host `GetDevice`, direct `d3d12.dll` | Cap silently incomplete | Compare ledger with footprint | State the cap is best-effort |
| R25 | Lock contention | Admission shares `xmem`'s lock, which walks all commits on virtual allocation | Slower allocation | Churn benchmark with many commits | Keep the walk off the charge path |
| R26 | Milestone delay | Memory work treated as part of an exit gate | M2 or M3 slips on an unproven idea | Review any gate wording change | No stage is added to a gate |
| R27 | Scope creep | "Observation" grows into behaviour change or a provider | Unreviewed architecture lands | Diff shows no HRESULT or allocation change | Keep 0a log-only |
| R28 | Confusing stages with milestones | Similar numbering | Wrong expectations | Review documents and issues | Always say "stage" or "milestone" |
| R29 | Parallel edits to claimed files | #486, #471, #483, #496 touch candidate files | Rework or takeover | Claim check before any work package | New files; wait for owners |
