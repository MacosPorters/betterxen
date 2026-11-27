# Validation strategy

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

A gate is met only by the evidence it names, reviewed by someone who did not write the code. A skipped or interrupted run is not a pass.

## Where each gate sits on the roadmap

These are proposal gates, **not** milestone exit gates. None is added to M0–M6.

| Stage gate | Milestone |
| --- | --- |
| 0a, 0b, 1, 2, 3, 4 | M3 at the earliest, after approval (3 and 4 tuning in M6) |
| 5 | M5, with M4 workloads |
| 6 | M6 |

## Gates by stage

### Stage 0a

| Item | Content |
| --- | --- |
| Objective | See graphics allocations and their destruction without changing behaviour |
| Dependencies | Coordination with #486 |
| Components | `d3d12_xs`, trace |
| Expected behaviour | Identical HRESULTs; would-reject and unseen-path records |
| Fixtures | Release heap while a placed resource lives; each creation path; GPU work in flight; device removal if obtainable |
| Negative controls | An uncounted path or an early release must fail the fixture |
| Regression | All conformance binaries |
| Independent review | Second operator and a second chip generation for the release signal |
| Rollback | Revert; nothing depends on it |
| Uncertainties | Sentinel timing and thread; paths outside the proxy |

### Stage 0b

| Item | Content |
| --- | --- |
| Objective | The cap holds in any order and under concurrency for visible paths |
| Dependencies | 0a results; `[ABI]` change, or the interim in [ABI impact](abi-impact-analysis.md) |
| Components | `xmem`, `d3d12_xs`, launcher switch |
| Expected behaviour | Over-cap requests fail with a logged reason; host failures roll back |
| Fixtures | Both allocation orders; concurrent reserve; the research 6 + 3 over cap 8 schedule, scaled |
| Negative controls | Removing the lock or a path must fail the fixture |
| Regression | `test_xmem`, `test_xmem_physical`, `test_d3d12_xs`, `test_integration_boot`, export verification |
| Independent review | Re-derive ledger transitions |
| Hardware | Two generations |
| Rollback | Switch off, then revert |
| Uncertainties | Whether host allocation sizes resemble console ones; new failures |

### Stage 1

| Item | Content |
| --- | --- |
| Objective | Provider boundary and diagnostics with no behaviour change |
| Dependencies | Stage 0a (and 0b if taken) |
| Components | `d3d12_xs`, `bx-telemetry`, `bx-cli trace` |
| Expected behaviour | Identical results and HRESULTs; reason codes in the trace |
| Fixtures | Forwarded-call HRESULT and descriptor parity with the host device |
| Negative controls | A deliberately wrong reason code or missing event fails the trace check |
| Regression | All conformance binaries |
| Independent review | Confirm no allocation path changed |
| Hardware | One CrossOver D3DMetal host plus a second reviewer |
| Rollback | Revert |
| Uncertainties | Logging cost on allocation-heavy workloads |

### Stage 2

| Item | Content |
| --- | --- |
| Objective | A bounded opt-in mapped-buffer path |
| Dependencies | Stage 1; decision D1; reviewed device-loss behaviour |
| Components | `d3d12_xs`; possibly `xmem` |
| Expected behaviour | Eligible requests use it; all others unchanged; ineligible explicit requests fail |
| Fixtures | Fence-separated CPU↔GPU round trips; alignment remainders; lifetime across reconfigure and device recreation |
| Negative controls | Omitted dispatch leaves poison; omitted wait is not exercised on the GPU, only in a policy test |
| Regression | Switch off must reproduce Stage 1 exactly |
| Independent review | Expectations from public contracts; independent hardware run |
| Hardware | M4 and M5 at minimum; report any M1–M3 as untested |
| Rollback | Switch off, then revert |
| Uncertainties | Physical sharing; driver retention; behaviour on other versions |

### Stage 3

| Item | Content |
| --- | --- |
| Objective | Reuse with correct accounting and retirement |
| Dependencies | Stage 2; licence and registry entries |
| Components | Pool provider, vendored allocator if chosen |
| Expected behaviour | Bounded blocks; no reuse before completion |
| Fixtures | Seeded churn; fragmentation with failing large requests; delayed-fence withholding; alignment |
| Negative controls | Premature recycle detected by guard data in a CPU-side policy test |
| Regression | Stage 0a–2 suites |
| Independent review | Upstream files and notices; span arithmetic |
| Hardware | Two generations; one 16 GB host under normal pressure |
| Rollback | Disable pooling, keep the unpooled provider |
| Uncertainties | Cold setup, slack, retained blocks, multi-queue |

### Stage 4

| Item | Content |
| --- | --- |
| Objective | Fewer or cheaper transfers, measured |
| Dependencies | Stage 1; Stage 2 only for mapped destinations |
| Components | `xcrdapi`, transfer helpers |
| Expected behaviour | Identical bytes; fewer created objects or copied bytes |
| Fixtures | Batched requests into upload, default and mapped buffers; cancellation and failure |
| Negative controls | Dropped copy and truncated range must fail byte checks |
| Regression | `test_xcrdapi`, including both interface sets after #483 |
| Independent review | Re-derive payloads and timing boundaries |
| Hardware | As Stage 3 |
| Rollback | Per-optimization revert |
| Uncertainties | Costs moved outside timed windows |

### Stage 5

| Item | Content |
| --- | --- |
| Objective | Stability and correctness under stress |
| Dependencies | Stages 2–4 as applicable; supervised runs (#407) |
| Components | Soak and failure harnesses |
| Expected behaviour | Bounded memory, no lifetime violations, clean failure on device loss |
| Fixtures | Long-run churn, multi-queue producer/consumer, forced device removal where possible |
| Negative controls | Injected leak and injected over-cap must be detected |
| Regression | Everything above |
| Independent review | Second operator, own receipts |
| Hardware | As many generations as available; list the untested ones |
| Rollback | Keep default off |
| Uncertainties | Real title behaviour: not testable with synthetic work |

### Stage 6

| Item | Content |
| --- | --- |
| Objective | Decide to enable, revise or reject |
| Dependencies | Stage 5 results |
| Expected behaviour | A written decision citing evidence and regressions |
| Rollback | Default stays off unless explicitly changed |
| Uncertainties | Whether synthetic results transfer |

## Benchmarks and how a claim would be measured

| Benchmark | Measure |
| --- | --- |
| Allocation churn | Per-call and whole-cycle time, with cold setup reported separately |
| CPU/GPU visibility | Byte-exact result counts |
| Explicit transfer volume | Bytes copied by BetterXen per unit of work |
| Memory pressure | Footprint, RSS, compression and swap, sampled separately |
| Fragmentation | Free total, largest span, failed requests |
| Lifetime correctness | Violations detected (expected zero) |
| Concurrent admission | Maximum observed total against cap |
| Multi-queue, device loss | Pass/fail with the injected condition named |
| Long-run stability | Hours, frames, ledger drift |

A performance claim needs: the same workload and data on old and new paths; separate CPU, encode, submit-to-fence and device intervals; several fresh processes in alternating order; full distributions including outliers; stated host, versions and pressure; the cost of setup included; and a second person reproducing it. Follow [the measurement protocol](../../../../testing/hybrid-workloads.md). No FPS or memory-saving figure is predicted.

## Safety limits for any run

One GPU workload at a time, at most four build jobs, check memory pressure first, stop and record rather than push through.

## Appendix: destruction signal probe

Full source, commands and logs: [evidence record](evidence.md).

### Summary

Original synthetic program run for this draft. **One host, author-run, three runs, not independently reproduced.** Apple M4 24 GiB, macOS 27.0, CrossOver 26.3 with D3DMetal. It attaches a counting `IUnknown` with `SetPrivateDataInterface`, releases the object and watches the count (2 while held, 1 once the runtime drops it), waiting 200 ms after each release.

| Case | Observed on all three runs |
| --- | --- |
| Plain `UPLOAD` heap | Sentinel released after the heap's last `Release` |
| Heap released while a placed buffer lives | `Release` returned 0; sentinel **still held**; released only after the buffer |
| Committed `UPLOAD` buffer | Sentinel released after `Release` |
| `ID3DDestructionNotifier` | `QueryInterface` returned `E_NOINTERFACE` |

Not covered: `DEFAULT` heaps, textures, GPU work in flight, device removal, other hosts or versions. The core of the placed case:

```cpp
dev->CreateHeap(&hd, __uuidof(ID3D12Heap), (void**)&h);
h->SetPrivateDataInterface(kKey, &sentinel);          // sentinel.refs == 2
dev->CreatePlacedResource(h, 0, &bd, D3D12_RESOURCE_STATE_GENERIC_READ,
                          nullptr, __uuidof(ID3D12Resource), (void**)&r);
ULONG left = h->Release(); Sleep(200);                // left == 0, refs == 2
r->Release();             Sleep(200);                 // refs == 1
```

SHA-256 of the full uncommitted source: `b6eb44d201986fd1eecf53939805d5ac11f14082d8eb57466d6ac53303ca554f`.
