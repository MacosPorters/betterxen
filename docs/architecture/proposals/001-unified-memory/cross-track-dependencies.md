# Cross-track dependencies

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

State as of 2026-11-26. Nothing here reassigns or blocks anyone's work.

| Track | Change needed | Can continue independently? |
| --- | --- | --- |
| ABI gate | Charge exports (Stage 0b); provider switch (Stage 2) | — |
| A — launcher, runtime | Parse new trace events; pass any new switch | Yes |
| B — `xmem`, DirectStorage | Own the budget authority; later, adopted ranges | #483 yes |
| C — graphics | Creation-path coverage, release signal, provider | Presentation yes; #486 overlaps Stages 0a and 0b |
| D — shaders | None | **Yes** |
| E — runtime services | None | Yes |
| F — tests, docs | Budget, lifetime and failure fixtures | Yes |

The possible [`XMemVirtualAlloc` parameter mismatch](eligibility.md#a-discrepancy-on-main-that-needs-maintainer-attention) belongs to Track B and would touch `test_xmem` and `test_integration_boot`. It is unclaimed.

## Can shader work continue in parallel?

Yes. The shader gate acts at pipeline creation and translation produces bytecode; neither allocates guest memory. The only contact is textual: both areas edit `shims/d3d12_xs/proxy.cpp` and its `CMakeLists.txt`. Keep provider code in new files.

## Related open work

| Item | Owner | Interaction | Suggested handling |
| --- | --- | --- | --- |
| [#464](https://github.com/MacosPorters/betterxen/issues/464) / [PR #486](https://github.com/MacosPorters/betterxen/pull/486) | @devil8blade | Implements heap charging and the first opt-in backing path | Let it finish on its own scope. Ask whether charging should wait for the authority ([D3](maintainer-decisions.md#d3-what-happens-to-486)). Do not build a parallel provider |
| [#465](https://github.com/MacosPorters/betterxen/issues/465) / [PR #471](https://github.com/MacosPorters/betterxen/pull/471) | @Arths17 | Back buffers are uncounted; shares `proxy.cpp` with #486 | Independent; decide back-buffer accounting in Stage 0a |
| [#469](https://github.com/MacosPorters/betterxen/issues/469) / [PR #494](https://github.com/MacosPorters/betterxen/pull/494) | @atesahmet0 | Exit gate depends on #486 and #471 | Independent of this proposal; see its review about `xmem` backing |
| [#483](https://github.com/MacosPorters/betterxen/issues/483) | @1Bartosz (claimed) | DirectStorage destinations are D3D12 resources | Independent; Stage 4 must keep both interface sets working |
| [#450](https://github.com/MacosPorters/betterxen/issues/450) | @soggyramennoodle | Known swap-chain retention | Relevant to lifetime diagnostics |
| [#407](https://github.com/MacosPorters/betterxen/issues/407) | @Arths17, @jelly013 | Supervision and evidence records | Stage 5 relies on supervised failure handling |
| ADR 0008 | maintainers | Shader branch decision | Orthogonal |
