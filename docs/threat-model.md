# Shader input and execution limits

## Scope

The limits below describe the preserved native fixture/compiler path. They are not a sandbox or security guarantee for CrossOver, Wine, D3DMetal or the vendored shader parser. Hybrid launches add an external process, bottle, selected backend and guest workload; assess those boundaries separately for the exact installed versions. Runner timeouts and resource supervision do not establish isolation or recovery from GPU stalls. The [architecture guide](architecture.md) records which launch dependencies are present.

## Native fixture boundaries

The public demo runs embedded, reviewed synthetic fixtures. The compiler library accepts arbitrary byte slices and requires an explicit gfx1030 target. It checks size and alignment before decoding and rejects unsupported forms, uninitialized registers and resource accesses beyond the fixture ABI. There is no container whose offsets can select host memory. An invalid stream cannot load arbitrary Metal source.

The Metal backend accepts trusted MSL under a two-buffer contract. It checks source size and dispatch dimensions, allocates disjoint buffers, dispatches one thread per word and waits for command completion before copying readback. It does not parse MSL. Treat the native API as a boundary for trusted source. The demo script bounds process runtime at 30 seconds and enables API and shader validation. A process timeout does not guarantee recovery from a hardware stall.

The fuzz harnesses exercise host-side parsers only. Corpus mutations do not reach the GPU. Logs, compiler diagnostics, and local artifacts can contain absolute paths and are kept outside tracked files. This original fixture flow has no capture replay or protected-content input path; it is not an inventory of all current workspace caches or parsers.
