# Final reciprocal verification findings

A reviewed B revision `4ba4926d2f491517c01b1e94fb8539184055dc22` and all three specified #473 comments in full. B independently reviewed A evidence at 8c9eceb and the prior consolidation at 677686e. The requester authorizes DCO-signed publication of this research update on the dedicated A branch; branch consolidation and production changes still require separate authorization. No production, ABI, main or frozen baseline source was changed. No Wine, Instruments or GPU workload was launched on M4 in this pass.

## Seven trace extractions: independently reproduced and accepted within scope

[Exact replay receipts](../../results/2026-11-26-apple-m4-arths17-final-trace-replay/receipt.json) retain all seven command arrays, input/extractor SHA-256 identities and complete output JSON. Every unmodified B extractor invocation exits 0 and produces JSON equal to the corresponding published result. A's separate XML reader resolves references, checks only the declared guest PID, schema columns, unique encoder/command-buffer associations and CPU/GPU chronological order. Duration numbers agree with the XML's formatted units at their displayed precision. Payloads are 4 MiB shader workloads versus 64 MiB copy workloads, and bytes/ns correctly yields decimal GB/s. All measured medians, median absolute deviations, extrema and rates recalculate; warmups and outliers remain present.

| Capture | PID | Selected intervals | Excluded target encoder rows |
| --- | ---: | ---: | ---: |
| WB compute | 60493 | 75 | 0 |
| WC compute | 60634 | 75 | 0 |
| UPLOAD→WB compute | 60758 | 75 | 0 |
| DEFAULT compute | 60843 | 75 | 270 staging/readback blits |
| WB full copy | 60920 | 15 | 0 |
| WC full copy | 61014 | 15 | 0 |
| UPLOAD copy retry | 61276 | 15 | 0 |

B-7 is independently accepted **for deterministic extraction from these supplied sanitized XML tables**. This goes beyond the previous arithmetic-only check. It does not independently recapture the GPU, establish completeness against the unavailable full unfiltered trace, identify physical transactions or establish intrinsic heap rankings. The sanitizer source and its declared full-export hashes support provenance, but A did not obtain the original full traces.

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/final-verification/verify-traces.py /path/to/detached/B-4ba4926 --output /tmp/fresh-trace-replay
```

## Stopped-run forensics and corrected harness

The original historical receipt/output are unchanged. A reconfirms the empty declared stderr digest versus the handshake-error bytes. B's published APFS table places the stderr modification 3,656,510 ns after case.json; A verifies that subtraction, while attributing the filesystem timestamps to B's record rather than an independent query of B's host.

A's [real Python process fixture](../../results/2026-11-26-apple-m4-arths17-final-process-sealing-clean/receipt.json) independently reproduces the same empty-to-handshake SHA transition with old B `run_case`: the launcher group exits while a separately grouped child waits on inherited stdin; sealing precedes scope-exit EOF and the child's final stderr write. Against B-local-1.1, normal and forced-stop fixture outputs have matching recorded/final hashes, and the after-guest callback finalizes its simulated wrapper log before sealing. This supports the late-writer/finalization mechanism and specific correction. It is not a real Wine or historical APFS replay; Wine cleanup calls are deliberately simulated and no graphics API is involved.

One hardening issue remains: the normal-path `wineserver -w` result is discarded. A controlled return of exit 124 still leaves `exit=0` and no error. Error-path cleanup results are retained but their failures are not enforced either. Therefore receipt finality is not established when cleanup fails/times out. B's real-Wine forced-stop verification is reported in prose, but its original result/command transcript is not present in this pin. A requested that minimal record. B-8 remains Failed for the historical sealed receipt, with the cause/source mechanism now independently supported and this further failure-path limitation explicit. No evidence implicates completed benchmark guest/shader outputs; wrapper xctrace logs have separate mid-save hashes/redaction records.

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/final-verification/verify-harness.py /path/to/detached/B-4ba4926 --output /tmp/fresh-process-review
```

The first A fixture setup attempt failed before launching any child because the old module's path was too shallow for its original relative-directory calculation; the corrected fixture restores that layout. This setup failure is distinct from all process/GPU outcomes.

## Peer decisions and runtime-test limits

B explicitly accepts BASE-1/3/4/5/6, A-3/4/6/8/9/10/13 within its stated scope. The register credits those twelve decisions to B; A does not certify its own components. BASE-2 remains version-scoped Unsupported and A-7 retains the failed negative-rejection gate. A independently checks B's published source/logs and regenerates thirteen expected graphics hashes; the [review receipt](../../results/2026-11-26-apple-m4-arths17-final-source-data-review/peer-evidence-review.json) records exactly what was checked.

The shared baseline source and full-run exit/timeout records match; B's lsof evidence is from a **separate receipt-only process**, not the full benchmark PID. This closes exact shared-runner functional cross-generation reproduction while retaining that backend-provenance distinction and baseline timing limits.

A-4's M5 reproduction is a WB/WC seed-473 pair using the corrected helper, not six M5 cases or byte-identical historical helper execution. All six M4 logs receive B's source/data review. A-3's independent bitmap checker and M5 rerun are reported by B but their source/raw reproduction are not in this pin; the register attributes them rather than claiming A ran that checker. Split remainders are visited in vector/split-age order between sorting/coalescing operations, and rounding assumes the bounded input domain.

B's A-13 fixture has a real positive queued gated-copy and two exit/canary controls. The Signal control **directly invokes stopInFlight**, rather than inducing a real queue Signal failure. The timeout uses an empty unsignaled fence with no stalled GPU work. Logs match the result JSON, exit codes are 0/2/2, and the stack canary does not run on failure exits. The fault bundle supplies source/logs and a peer attribution to the exact helper, but no dedicated full build/PE-hash receipt; A does not claim to verify a missing executable identity chain. B accepts the research helper for single-threaded disposable guests; production multithreaded/DLL-detach, device-removal and arbitrary failed API supervision remain separate. M4 pressure is still a safety blocker, but B helper review is no longer a blocker to later safe dependent runs.

A-12 remains incomplete. Raw stdout shows M5 variant1 completed both depth comparisons and then emitted **STAGE stencil** before the urgent-pressure stop, correcting the review comment's “depth” wording. B publishes frame SHA manifests and omitted-control oracle metadata, not raw RGBA payloads. A's independent expected-byte hashes all match the thirteen provided hashes; this is not direct inspection of the missing frame bytes. Variant0 and omitted draw are scoped reproductions, while the two-dataset matrix stays Blocked/Unknown.

## Independently confirmed combined-budget defect

A inspected exact sources at PR #486 revision `c13f9c76650edded41774528b50f40798244a6f3`; its vmem files are unchanged from c44ae4e. Source fingerprints and a bounded [arithmetic counterexample](../../results/2026-11-26-apple-m4-arths17-final-source-data-review/budget-review.json) are retained. This is source-derived reasoning, not production execution or a physical-memory measurement.

`shims/xmem/xmem.cpp:73` admits requests against heap+virtual+physical bytes under `usage_lock`. Both virtual commits and physical reservations use that authority, and heap allocations do too. PR `vmem.cpp:101` takes `g_section`, obtains an xmem statistics snapshot and additionally counts its private live heaps. No reverse private-heap visibility exists in xmem admission. Its exported committed statistic already includes xmem heap bytes, so the defect is not an omission of that particular field; it is missing reverse visibility and atomic combined admission.

With a stable common cap of 8 abstract units, a distinct private heap of 6 can succeed with xmem=0; then an xmem request of 3 sees xmem=0 and succeeds, yielding combined live storage 9. Reverse order rejects the heap. A concurrent source-permitted schedule starts with xmem4, snapshots4 for the graphics request, commits another CPU4 and then graphics4 from the stale snapshot, yielding12 against8. These examples assume successful underlying allocations and no aliases; no GiB buffers are allocated. R3 must address correctness/atomicity, not merely display summed counters. Source proof does not replace backend regression tests or authorize a fix, and does not certify/reject the entire PR.

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/final-verification/verify-budget.py /path/to/read-only/c13f9c7-export --output /tmp/budget-review.json
```

## Status boundary

Thirteen register entries have scoped independent acceptance: twelve explicit B peer decisions and A's supplied-input trace extraction decision. Unsupported imports, failed rejection/timestamp/receipt gates and Unknown physical mechanisms remain distinct. M4 B hardware reproduction, graphics repetition/omitted draw and missing footprint sessions remain urgent-pressure blocked. B's bitmap/raw-frame/forensic transcript packaging and finalization failure propagation are review follow-ups, not memory-pressure blockers to CPU work. No physical zero-copy, guaranteed production speedup or game compatibility is claimed.
