# Joint evidence register

A consolidates the register under its owned directory. The register incorporates B's explicit scoped decisions and A's CPU verification; B [signed off this content at d3ae607](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7206014568). Integration changed no finding record (see the [integration record](../../integration.md)). It combines pinned evidence without changing either workstream's raw results. No entry is independently accepted merely because agents agree. The [complete machine-readable register](evidence-register.json) contains every field for each finding, including hardware/runtime, exact source revision, raw links, review/reproduction attribution, limitations and architectural relevance.

The A pin is `8c9eceb05ab660504ee981c2f4fcaf1728a5dbff`; historical B benchmark source is `a40228747470f0d184375e451b0173769c90aef5` and B review/trace inputs are `4ba4926d2f491517c01b1e94fb8539184055dc22`; the six-check baseline is `9ae6d1ee7092b6021aad075531f50ba60de4fd53`. Exact executed snapshot hashes in receipts take precedence over a later helper revision. M4 has 24 GiB/macOS 27.0; M5 16 GiB/macOS 27.2.1; both use CrossOver 26.3.0.39832, Wine 11.0-8726-g2e2f5fca349 and D3DMetal 3.0, with the detailed differences recorded in JSON.

| ID | Research question | Owner | Evidence status | Acceptance |
| --- | --- | --- | --- | --- |
| BASE-1 | What architecture/heap capabilities are reported? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| BASE-2 | Can existing CPU address/file-mapping allocations become D3D12 heaps? | A | Author-tested; Independently source-reviewed; Independently reproduced; Unsupported | Unsupported |
| BASE-3 | Which CUSTOM combinations and initial mixed placement work? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| BASE-4 | Does fence-separated persistent-map copy visibility work? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| BASE-5 | Does original small-copy latency prove no shadow copy? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| BASE-6 | Does ordinary CPU MEM_WRITE_WATCH work? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-1 | What is process-accounted cost across 64/256/1024 MiB and complete retirement? | A | Author-tested; Independently source-reviewed; Blocked | Unknown |
| A-2 | Does bounded pooling improve matched allocation/retirement cost? | A | Author-tested; Independently source-reviewed | Unknown |
| A-3 | Can total free bytes hide fragmentation and pending-fence unavailability? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-4 | Does pinned AMD D3D12 Memory Allocator support bounded CUSTOM WB/WC buffers here? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-5 | Can inverted ownership satisfy every current xmem requirement? | A | Independently source-reviewed; Unknown; Failed | Unknown |
| A-6 | Are mapped CPU pointers guaranteed 64 KiB aligned? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-7 | Does creation success reject invalid resource placement? | A | Author-tested; Independently source-reviewed; Independently reproduced; Failed | Failed |
| A-8 | Do valid overlapping buffers inherit data with explicit ordering? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-9 | Do mixed opaque mip textures and disjoint guards transfer correctly? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-10 | Are render/depth aliases correct after activation and reinitialization? | A | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| A-11 | Do UNKNOWN and legal ROW_MAJOR controls have the same scope? | A | Author-tested; Independently source-reviewed | Unknown |
| A-12 | Do sampled rendering, raster depth and stencil work in the new fixture? | A | Author-tested; Independently source-reviewed; Independently reproduced; Blocked | Unknown |
| A-13 | Is fence failure safe without proven completion? | A | Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| B-1 | What hidden-copy costs can matched small/full copy windows bound? | B | Author-tested; Independently source-reviewed; Blocked | Unknown |
| B-2 | What bandwidth does the workload actually measure? | B | Author-tested; Independently source-reviewed; Blocked | Unknown |
| B-3 | Does direct SRV/UAV access preserve fence-separated visibility? | B | Author-tested; Independently source-reviewed; Blocked | Unknown |
| B-4 | Are 4 KiB subpages, reset/rearm, reads and same-value CPU stores tracked? | B | Author-tested; Independently source-reviewed; Blocked | Unknown |
| B-5 | Can the watch allocation be GPU-written through import? | B | Author-tested; Independently source-reviewed; Unsupported; Unknown | Unsupported |
| B-6 | Are D3D12 timestamp resolves reliable for these measurements? | B | Author-tested; Independently source-reviewed; Failed | Failed |
| B-7 | Do target-only Metal intervals support independently extracted GPU rates? | B | Author-tested; Independently source-reviewed; Independently reproduced; Independently accepted | Independently accepted |
| B-8 | Why does a stopped trace stderr not match its receipt hash? | B | Failed; Independently source-reviewed; Unknown | Failed |
| X-1 | Has physical zero-copy/page identity been established? | A+B | Unknown; Independently source-reviewed | Unknown |
| X-2 | Is generation/normal-pressure/general resource coverage sufficient? | A+B | Blocked; Unknown; Independently source-reviewed | Unknown |

## Coverage of the revised plan

| Original objective | Register coverage |
| --- | --- |
| Six frozen checks | BASE-1 through BASE-6 |
| A footprint and complete retirement | A-1 |
| A pooling, fragmentation and reuse | A-2 through A-4, A-13 |
| A allocator reuse/provenance | A-4 and pinned allocator assessment |
| A ownership/budget/placement | A-5 through A-8, A-13 |
| A texture/layout/storage correctness | A-9 through A-12, X-2 |
| B matched copy detection and bandwidth | B-1, B-2, B-7 |
| B shader visibility and synchronization | B-3, A-13, pinned contract notes |
| B CPU/GPU write tracking | B-4, B-5 |
| Reciprocal review, failures and measurement validity | B-6 through B-8, X-1, X-2 |

## Reading acceptance correctly

Unknown acceptance means the overall proposition has not received explicit independent acceptance; it does not erase recorded successful subcases. Failed describes the named rejection/timestamp/finalization gate, not a fabricated failure of valid GPU data. Unsupported is version/configuration scoped. Blocked identifies work that could not safely run or lacks review input. Same-source reproduction and independently authored probes remain distinct. Thirteen findings now have scoped independent acceptance: twelve credited to B's explicit decisions and B-7 accepted by A for CPU extraction of supplied XML. Their acceptance scopes in JSON are mandatory; none establishes physical zero-copy, general production performance or complete compatibility. A-12 remains incomplete, and A-5 records a source-proven combined-budget defect. Dedicated A-branch publication is authorized; no integration or production acceptance is implied.

See [verification details](../final-verification/findings.md) for seven exact XML replays, process-finalization controls, peer-artifact limits and the budget counterexamples. B review/input pin is `4ba4926d2f491517c01b1e94fb8539184055dc22`; original executed-source pins above remain historical.
