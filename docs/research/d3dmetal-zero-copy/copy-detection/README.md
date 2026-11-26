# Workstream B copy, memory and synchronization research

This supplement to [#473](https://github.com/MacosPorters/betterxen/issues/473) uses only synthetic buffers and the installed public D3D12 interfaces. The six baseline cases are frozen. See the [baseline](../README.md) and [measured findings](findings.md). [Contracts](../gpu-correctness/contracts/synchronization-mapping-tracking.md), the [initial independent A review](review-a.md), and the [continuation and consolidation review](review-a-consolidation.md) accompany these experiments. No production shim or ABI participates.

## Reproduce

Use a licensed CrossOver installation on Apple Silicon, Rosetta and a local win64 CrossOver bottle. The runner clones the bottle, records settings and source bottle configuration hash, and stops the private server on return. Each graphics case checks loaded D3DMetal DLL/framework paths before running the workload. Each case has a 45-second timeout and individual fence waits have a 5-second timeout. The host lock serializes runs from both workstreams when they use the common local lock path. Launch filters environment variables and copies only the external HLSL source into the compute guest's working directory.

```sh
probe_dir=docs/research/d3dmetal-zero-copy/copy-detection
python3 "$probe_dir/run.py" --group small --bottle "/path/to/win64/bottle" --scratch /tmp/b473-small
python3 "$probe_dir/run.py" --group compute --seed 473 --bottle "/path/to/win64/bottle" --scratch /tmp/b473-compute
python3 "$probe_dir/run.py" --group sweep --seed 473 --bottle "/path/to/win64/bottle" --scratch /tmp/b473-sweep
python3 "$probe_dir/summarize.py" /tmp/b473-compute /tmp/b473-sweep
```

Each scratch path must be new. A build-only invocation omits the bottle and uses `--group build`. Repeat with a new seed and fresh scratch directory. The default profile requires normal kernel pressure. A separately labeled `--allow-warning` case can run at Warning while still stopping at Urgent. Pressure and compression are recorded outside benchmark windows. Treat stopped and partial runs as incomplete. [XNU pressure levels](https://github.com/apple-oss-distributions/xnu/blob/main/doc/vm/memorystatus_notify.md).

## Measurements

- The copy sweep uses 1, 16 and 64 MiB with WB, WC and standard UPLOAD sources, a matching READBACK output, and 4 KiB or 16 KiB write strides. CUSTOM buffers begin in COMMON and use legal promotion/decay. UPLOAD remains GENERIC_READ. The workload uses the same READBACK destinations, command lists and fence path for each source heap.
- Each case has three warmups and twelve timed iterations. Case and page order use the receipt seed. The first three pages in the seeded order cover the first word, an interior word and the last stride's first word. Cases range from zero modified pages to one word per page in the measured allocation. Genuine whole-buffer rewrites are separate cases. Allocation, initialization and output poisoning are outside the write, encoding and submission intervals.
- Constant-size GPU work copies three words. Full GPU work copies the allocation and validates every word, including unchanged words inside modified pages. The full workload brackets the copy with D3D12 timestamp queries. GPU rates use useful copy payload bytes divided by query elapsed time. Empty command lists and signal-only waits provide host controls. Omitted copies keep poisoned output, including in the timestamped no-copy case.
- Record CPU write (including store fence), reset and encoding, Execute call, queue Signal call, fence arm/wait, end-to-end submission/completion, and validation separately. The CPU bandwidth loop measures whole-allocation memset and a one-stream checksum scan after that write. Report these as effective workload rates with cache and prior-write effects.
- The separate [compute and tracking fixture](../gpu-correctness/coherence-and-tracking/README.md) measures direct shader access, wide reads with compact output, sparse reads and store-only throughput. Compiled DXBC remains in the scratch receipt with its SHA-256.

Compare excess elapsed time and variation across fixed-workload slices, sizes, write counts and strides separately. Sparse CPU writes can change cache ownership, trigger first access or write-combine effects, and create synchronization overhead without a byte-for-byte copy. Full-buffer reads can move data inside a backend before the first GPU counter query or during later work outside it. The elapsed-time distributions can constrain added cost in the measured scenario. They cannot identify physical page identity, pin a copy to a stage based only on correlation, or exclude work that is deferred, overlapped, restricted to referenced ranges, or charged elsewhere. No theoretical Apple memory bandwidth is assumed.

One host run can expose a regression or specific measured behavior. A stable comparison needs repeat sessions, reported dispersion and actual backend verification. Timestamp results at or below zero are reported as unavailable. Query overhead is visible in the timestamped omitted-copy case. Graphics pipeline and validation work, cache effects and the admitted host pressure limit GB/s comparisons to the defined workloads. [D3D12 timing](https://learn.microsoft.com/en-us/windows/win32/direct3d12/timing).

## Device-side timing with Instruments

The tested D3D12 timestamp resolves leave the output unchanged. Use a separate Metal System Trace recording to time GPU work. This needs the installed Apple Xcode and its Instruments permissions. The wrapper attaches to the verified synthetic guest process before releasing the backend handshake. Keep traced host timing apart from the uninstrumented measurements.

```sh
probe_dir=docs/research/d3dmetal-zero-copy/copy-detection
scratch=/tmp/b473-metal-wb
python3 "$probe_dir/trace.py" wb --group compute --seed 947 --bottle "/path/to/win64/bottle" --scratch "$scratch"
trace=$(find "$scratch" -maxdepth 2 -name 'gpu.trace' -print)
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
xcrun xctrace export --input "$trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]' --output "$scratch/gpu-intervals.xml"
xcrun xctrace export --input "$trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="metal-application-encoders-list"]' --output "$scratch/metal-encoders.xml"
python3 "$probe_dir/summarize-trace.py" "$scratch" "$scratch/gpu-intervals.xml" "$scratch/metal-encoders.xml"
```

Repeat serially with `wc`, `upload-wb`, `default`, `copy-wb`, `copy-wc`, or `copy-upload` as the wrapper's first argument and use a fresh scratch directory each time. The copy workload moves 64 MiB to READBACK, compares every word, and varies the two endpoints. All traced inputs retain three warmups and twelve measured iterations in their own process.

The parser requires a complete, unique sequence of matching Metal command-buffer and encoder IDs for the verified probe PID, along with successful output oracles and matching guest/encoder/GPU interval counts. For the shader it uses active Compute intervals and excludes staging and omitted dispatches. The denominator is the GPU interval duration. Report useful bytes per interval, which includes shader work and cache effects. It does not count physical bus traffic. The full trace and XML exports stay in the local scratch directory because the template can include unrelated host processes. The archived JSON extracts contain only the synthetic guest's matched intervals, and sanitized target-PID-only XML exports retaining referenced XML nodes are archived in [`2026-11-27-apple-m5-atesahmet0-b-trace-inputs-01`](../results/2026-11-27-apple-m5-atesahmet0-b-trace-inputs-01/receipt.json) via [`export-target-trace.py`](export-target-trace.py); see the [B1/B2 audit](b1-b2-audit.md). [Apple Instruments Metal analysis](https://developer.apple.com/documentation/xcode/analyzing-the-performance-of-your-metal-app).
