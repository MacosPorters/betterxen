# CrossOver hybrid architecture and integration boundaries

[ADR 0005](adr/0005-crossover-hybrid-direction.md) establishes CrossOver as the
primary compatibility foundation and hybrid as the only active development
track. The [roadmap](roadmap.md) defines phase acceptance. This guide describes
component boundaries without treating the proposed complete stack as verified.

## Responsibilities

BetterXen owns integration, configuration, environment inspection, diagnostics,
reproducible workloads, run records, benchmarking and measured optimization.
CrossOver is installed externally. Its Wine-based runtime and selected CPU and
graphics translation components are upstream responsibilities. Record installed
versions and supported interfaces; an observed environment variable or bottle
setting is not automatically a supported vendor API.

BetterXen's own host tools must run natively as ARM64. Windows x86-64 test
executables are guest workloads, not BetterXen host binaries. Rosetta 2 may be
used by the selected external stack where supported. Metal is the host graphics
API; a backend using Vulkan through MoltenVK must be assessed as that backend's
path rather than an independent BetterXen graphics architecture.

CrossOver, D3DMetal, Rosetta 2 and Apple tools are not bundled. Their exact terms,
versions and support status must be checked. See the [upstream registry](research/upstream-registry.md),
[reuse matrix](research/component-reuse-matrix.md) and [license review](licenses.md).

## What is present in the split integration stack

This documentation branch is stacked on [PR #395](https://github.com/MacosPorters/betterxen/pull/395),
which is based on the shader-crate [PR #394](https://github.com/MacosPorters/betterxen/pull/394).
That base contains:

- `victory-shader`: a pinned upstream GFX10-to-DXIL compiler, with a BetterXen
  entry point for original compute machine code and root-layout inputs.
- Original compute-fixture tests that compare output with the fixture manifest.
- `scripts/hybrid/run-shader-gpu-tests.sh`: a Windows cross-build and test-launch
  script. It delegates execution and run records to `betterxen-hybrid`.

The `betterxen-hybrid` crate is supplied separately by
[PR #384](https://github.com/MacosPorters/betterxen/pull/384), with stall supervision
in [PR #388](https://github.com/MacosPorters/betterxen/pull/388). Neither is in
#395's base. The script cannot run from this stack alone: its first build of
`betterxen-hybrid` fails if that package is absent. Use a checkout containing
those dependencies before following the [hybrid test procedure](testing/README.md).
No merge, rebase or combined implementation is performed by this documentation PR.

With those dependencies available, the tested fixture path is:

```text
Original gfx1030 compute fixtures + explicit root layout
    -> victory-shader -> PC DXIL -> Windows D3D12 test executable
    -> external CrossOver / selected D3D12 backend -> GPU readback
    -> comparison with independently derived manifest words
```

The [integration report](research/victory-shader-integration.md) records local
Apple M4/CrossOver observations, six compute fixtures with matching output and
the [wave-operation investigation and fixes](research/shader-wave-semantics.md)
now included in this stack. The original two suite failures and the later
229-passed/3-ignored run remain separate records. Independent reproduction is outstanding. Windows
GPU tests are not compiled by the macOS test run, and upstream tests can report
success after skipping GPU work. These results do not demonstrate console
shader-container compatibility, Xbox startup, game execution or a performance gain.
The wave investigation began in [PR #392](https://github.com/MacosPorters/betterxen/pull/392)
and is now included through #394/#395. Its 64-lane-only test checks rejection on
the recorded 32-lane device; that passing test is not a GPU sum verification.
The corrected fp64 sum has recorded local GPU comparisons, with console tree
recognition and performance limits still documented in the investigation.

## Preserved native research

The independent decoder, x86-64 interpreter and ARM64 translator, synthetic
runtime services, gfx1030-to-MSL compiler, Metal backend and Swift app remain in
the workspace. Their contracts and tests remain valid for their documented
synthetic scope. They are not the CrossOver runtime and their coverage counts
are not hybrid compatibility metrics.

[ADRs 0003](adr/0003-demo-memory-model.md) and
[0004](adr/0004-cpu-boundary.md) retain native fixture and dependency contracts.
[Interfaces](interfaces.md), [native progress](progress.md) and historical
acceptance records retain their original evidence scope. Earlier native phases,
G-number gates, X-number experiments and CPU campaigns do not define active
hybrid milestones. See the [deferred native roadmap](research/native-roadmap.md).

Keep native regression tests passing. Expand native components only for a
specific demonstrated hybrid need or an explicit maintainer decision to resume.
Do not open a parallel native roadmap or adopt other upstream components merely
because a shader snapshot was included.
