# Evidence and hallucination audit

## Did the AI agents hallucinate any of these research findings?

**This audit found no evidence that any experiment or numerical result was invented.** Every published figure it recomputed matched the raw logs.

It did find claims that went further than their evidence, one wrong summary written by the auditor, and some stale wording. It cannot show the research is free of hallucination. Logs and hashes can be consistent without proving that each historical GPU run happened as described, and some reported results have no published raw data.

## Who audited, and how independent it is

Claude Code, working for Arths17, on 2026-11-26. The same agent integrated the two workstreams, so for the integration record this is self-checking, not independent review. Workstreams A and B, their reviews and this presentation were all largely AI-assisted: Codex and Claude Code agents on Workstream A, an AI research agent on Workstream B, and Claude Code for 2elliti's early reproduction. **Nobody has independently reviewed this audit.** Agreement between agents was not treated as evidence.

## What was done

- Parsed raw stdout with new code ([`scripts/recompute.py`](scripts/recompute.py)), not the workstreams' summarizers: 36 copy processes, 8 compute processes, baseline, memory, pooling, fragmentation, texture, alias, graphics and write-watch logs.
- Re-derived the first expected word of every copy sample, and the first and last expected words of all 720 compute samples, from the probe and shader source.
- Matched 2,124 Workstream A archive digests, 109 Workstream B source snapshots and 165 Workstream B output digests (156 matched).
- Re-ran the CPU-only allocator analysis and the budget counterexample script; read the budget source at `c13f9c7` and on `main`.
- Read the probe sources for copy and compute, the findings documents, and the relevant #473, #464 and PR #486 comments.

**Not done:** no GPU, Instruments or Wine run; the process-finalization fixture and trace extraction were not re-executed (the archived replay output was compared with the published values); upstream allocator files and Microsoft documentation were not re-read; sources not named above were not re-reviewed line by line; backend receipts were checked programmatically only for the copy and compute runs.

Source revision for every row below is `f8810a48` unless stated.

## Audit table

Verdicts: **Verified**, **Independently reproduced** (documented separate execution), **Supported within scope**, **Inference**, **Unverified**, **Misleading**, **Incorrect**, **Fabricated**.

| Claim | Original evidence | Verification performed | Verdict | Limitations |
| --- | --- | --- | --- | --- |
| Existing-memory imports return `E_NOTIMPL` | Baseline logs, M4 and M5 | Logs read; non-timing lines identical | Independently reproduced | Same source on both; tested versions only |
| Five L0 custom heaps created, L1 rejected | Baseline logs | Same line comparison | Independently reproduced | Creation only for restricted heap types |
| Fenced persistent-map copies match | Baseline logs | Same | Independently reproduced | Not concurrent access |
| Flat small-copy latency does not prove no copy | Probe source, BASE-5 | Reasoning checked: 128 bytes per submit | Verified | A negative conclusion only |
| CPU Map pointers are not reliably 64 KiB aligned | Memory, placement, alias logs | Remainders recomputed or read | Independently reproduced | Observed values, not a distribution |
| Invalid placements return success | Texture logs, placement logs | Read on both hosts | Independently reproduced | Not shown to be a runtime bug |
| Alias buffers inherit data with a barrier | Alias logs | All match counts checked | Independently reproduced | Same source on M5 |
| Valid mip transfers and RT/depth clears match | Texture logs | All 17 outcome lines per session fully matched | Independently reproduced | Process exits 1 at the rejection gate |
| Sampling, depth, stencil work | Render logs | Match counts and stop stage read | Supported within scope | Matrix incomplete; raw frames unpublished |
| M5 reviewer checked all seven packed `.rgba` frames | Review text | Frame bytes absent | Unverified | Logged match counts do agree |
| 5,040 copy comparisons matched | 36 stdout logs | Recounted; expected word re-derived | Verified | M5 only; one source |
| 576 compute comparisons matched | 8 stdout logs | Recounted; all 720 samples re-derived | Verified | Endpoint words re-derived; full-buffer match counts as logged |
| An independent expression regenerated 720 samples | B findings text | Script unpublished; result re-derived here | Unverified (that run); result Verified | |
| Whole rewrite adds 46 to 279 µs | Sweep logs | Recomputed: 279, 197, 196, 46 | Verified | Baselines differ between sessions |
| Small copies 181 to 401 µs, full copy 2.09 to 2.36 ms (64 MiB) | Sweep logs | Recomputed | Verified | End-to-end medians |
| Copy results rule out hidden copies | Early comments | Compared with method | Misleading | Only a bound on one timed window |
| CPU read/write and fence-interval rate ranges | Sweep logs | Recomputed | Verified | Effective workload rates |
| Metal trace medians and rates | `gpu-samples.json`, sanitized XML | Compared with archived replay; tables checked | Verified | One capture per case; XML not recaptured |
| Seven extractions reproduce | A's replay bundle | Replay JSON equals published JSON | Independently reproduced | Extraction, not measurement |
| Timestamp resolves unavailable | Clock logs | Read | Supported within scope | Cause unknown |
| Write-watch gives exact 4 KiB page sets | Watch logs | 33 phases: actual equals expected | Verified | CPU writes only |
| GPU write tracking is impossible | Not claimed in final report | Searched final documents | Unverified (unknown) | `IsolatedMMU=0` removes a guarantee only |
| Footprint table (64 to 1024 MiB) | Memory logs | Recomputed | Verified | One complete session |
| Released large allocations are retained | Same | Same | Supported within scope | Accounting, not residency; not shown to be a leak |
| Pooling medians and setup cost | Pool logs | Recomputed | Verified | Author-tested only |
| An earlier pooling pair had the opposite ordering | A findings | Not recomputed | Unverified here | Raw bundle is published |
| 6,012 fragmentation snapshots; 180, 93, 142 failed requests | JSON lines | Recounted; conservation checked | Verified | Logical model; M5 check is B's |
| AMD allocator: 432 requests, 36 statistics, 24 readbacks per run | M4 bundle | Analysis re-run for six cases | Verified | M5 subset not re-examined here |
| AMD allocator guarantees a speedup | Not claimed | Searched final documents | Unverified | No such result exists |
| Fence-failure probe exits without recycling | B fault bundle | Result JSON read | Supported within scope | `Signal` failure was simulated by calling the handler |
| Late stderr write caused the stale digest | B timestamps, A fixture | Records read, not re-run | Independently reproduced | Mechanism with plain processes; original timestamps are B's |
| Combined budget can exceed the cap | Source at `c13f9c7` | Source read; script re-run | Verified (source) | Not run in production |
| "A checked 98 source identities and 1,080 endpoint pairs" | A review bundle | Not re-run; 109 snapshots matched here | Unverified here | |
| M5 1 GiB stages and dirty-page scaling to 256 MiB (early #473 comment) | Comment tables only | No source or raw distributions published | Unverified | Not part of the accepted findings |
| "True zero-copy" via `IOAccelerator SM=SHM`, 1x footprint, flat latency | Early #473 comment, #464 cross-post | Compared with what those measure | Misleading | See below |
| PR #486: buffers are "true shared memory ... no extra copy" | PR description | Compared with register X-1 | Misleading | Still present on 2026-11-26 |
| 2elliti's five M5 baseline runs | Comment table | No raw logs published | Unverified | Reported with its protocol differences stated by 2elliti |
| Physical zero-copy established | None | Register X-1 | Unverified (unknown) | |

**Fabricated: no row.** Missing evidence is recorded as Unverified, not as fabrication.

## How the zero-copy claim was narrowed

1. Early on 2026-11-26 a Workstream B comment stated that custom L0 heaps were "directly" shared memory and that "true zero-copy" worked, from a memory-region label, a footprint reading and flat latency. It was cross-posted to [#464](https://github.com/MacosPorters/betterxen/issues/464).
2. 2elliti's review noted that the baseline's latency check copies 128 bytes, so it could not detect a dirty-page copy, and later that physical sharing was unmeasured.
3. The research plan recorded that those signals do not prove page identity or exclude copies outside the window.
4. Workstream B [retracted the stronger inferences](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7203963689) and then ran full-copy controls with every byte checked.
5. The final register leaves physical identity and internal copying **unknown**.

The retraction is on #473. **No explicit retraction was found on #464, and the PR #486 description still uses the stronger wording.** This audit did not edit either.

## Specific checks requested

| Did previous agents... | Finding |
| --- | --- |
| Invent experiments or numbers? | No evidence of it |
| Report tests that never ran? | None found. Some reports have no published raw data (marked Unverified) |
| Present smoke tests as complete matrices? | No; the graphics smoke is labelled incomplete |
| Present same-source reruns as independent implementations? | Mostly labelled correctly. "Independently reproduced" in the register often means same source |
| Claim guaranteed physical zero-copy? | Yes, early; later retracted on #473 |
| Rule out internal copying? | The early claim did; the final report does not |
| Confuse footprint with residency, or call retention a leak? | Not in the final documents |
| Claim guaranteed allocator speedups? | No |
| Call GPU write tracking impossible? | No |
| Invent FPS or memory-saving percentages? | No |
| Extrapolate to untested hardware? | Not in the final documents |
| Claim missing artifacts were verified? | One review reports pixel checks of frames that are not published; the final report notes the absence |

## Errors and corrections found

| Error | Who | Status |
| --- | --- | --- |
| The integration record lists the unmatched Workstream B digests as one stderr file, two external allocator files and two trace tools. It omits **8 `xctrace-output.txt` digests that do not match** (taken while Instruments was still saving; recorded in [B's own audit](../../d3dmetal-zero-copy/copy-detection/b1-b2-audit.md)). The auditor's filter skipped that file name | Claude Code, in [`integration.md`](../../d3dmetal-zero-copy/integration.md) | Recorded here; the reviewed research branch is not edited |
| A diagnostic printed 16666.667 µs as GPU time from two untouched sentinel values | Workstream B probe | Found and corrected before the findings |
| A review says a graphics run stopped at the depth stage; the log shows stencil | Workstream B review | Corrected in the joint report and confirmed by B; historical text kept |
| An early probe skipped the write-back L0 case as "UMA-only" | 2elliti | Self-corrected |
| A graphics probe unloaded its shader compiler while still using it | Workstream A | Fixed; failed run kept |
| The BASE-1 register text says cross-generation runner reproduction is not established and, next sentence, that it is now recorded | Consolidation | Stale sentence, not corrected |
