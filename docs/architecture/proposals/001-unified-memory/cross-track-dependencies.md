# Cross-track dependencies

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

Track responsibilities, the component ownership matrix, the current issue table and the track diagrams are maintained in [roadmap integration](roadmap-integration.md) so there is one copy. Nothing here reassigns or blocks anyone's work.

| Track | Summary | Milestones with memory work |
| --- | --- | --- |
| ABI gate | Needed only for enforcement and later | M3, conditional |
| A | Trace events, switch, supervision | M2, M3, M5, M6 support |
| B | Authority, investigation, staging reuse | M1 follow-up, M2 support, M3–M6 |
| C | Observation, validation, any provider | M1 follow-up, M2–M6 |
| D | Independent | None |
| E | Independent | None |
| F | Fixtures and independent verification | M2–M6 |

The possible [`XMemVirtualAlloc` parameter mismatch](eligibility.md#a-discrepancy-on-main-that-needs-maintainer-attention) belongs to Track B and would touch `test_xmem` and `test_integration_boot`. It is unclaimed.

## Can shader work continue in parallel?

Yes. The shader gate acts at pipeline creation and translation produces bytecode; neither allocates guest memory. The only contact is textual: both areas edit `shims/d3d12_xs/proxy.cpp` and its `CMakeLists.txt`. Keep provider code in new files.

## Related open work

See [current issues and pull requests](roadmap-integration.md#current-issues-and-pull-requests). ADR 0008 (shader path) is orthogonal to this proposal; its file still reads Proposed.
