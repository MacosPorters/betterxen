# Crate guide

The active direction is [CrossOver-based hybrid integration](../docs/architecture.md).
A crate's presence in the workspace or its API documentation does not make it an
active development priority. Legacy `xenora-*` package names and guest ABI symbols
remain for compatibility; they do not name a second active architecture.

| Directory | Role | Development scope |
| --- | --- | --- |
| `victory-shader` | Vendored GFX10-to-DXIL compiler with an original compute-fixture bridge | Active hybrid integration; see [its guide](victory-shader/README.md) and [provenance](victory-shader/UPSTREAM.md). |
| `cli` | `betterxen` CLI for the bounded native experiments and inspection | Preserved native tooling; distinct from the separate `betterxen-hybrid` runner. |
| `loader` | PE32+ parsing, checked guest memory, x86-64 interpreter, ARM64 translator and synthetic services | Preserved, paused; not Wine or the CrossOver loader. |
| `import-gen` | Deterministic import classification and binding generation | Preserved native tooling; not proof of Xbox service compatibility. |
| `isa` | Typed gfx1030 instruction forms and generated ISA data | Preserved native shader path. |
| `decoder` | Bounded gfx1030 decoder | Preserved native shader path; separate from the vendored compiler's decoder. |
| `shader` | Synthetic gfx1030 compute/graphics to Metal Shading Language | Preserved, paused; not the DXIL bridge. |
| `ref` | Independent CPU expectations for native synthetic shader and rendering tests | Preserve independence from production implementations. |
| `metal` | Bounded trusted MSL execution, rendering and readback | Preserved native backend; not D3DMetal. |
| `texture` | Original synthetic texture layout and block-format helpers | Preserved native fixture support; not a verified Xbox layout. |
| `ffi` | Versioned C interface used by the Swift app | Preserved native application boundary; not CrossOver integration. |

`tests/validation` and `benchmarks` are additional workspace members for
independent native validation and correctness-gated measurements. The Swift app
is under `app/`. Keep their regressions passing; extending them requires an
explicit hybrid need or a maintainer decision.

`betterxen-hybrid` is not in this split stack's base. Its separate integration
PRs and the GPU script prerequisites are documented in the
[testing guide](../docs/testing/README.md). Use `cargo +1.100.0 doc --locked --workspace --no-deps` for crate API documentation; compiling documentation does not validate
GPU behavior or upstream provenance.
