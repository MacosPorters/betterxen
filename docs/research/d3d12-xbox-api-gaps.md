# Xbox D3D12 API gap: `CreatePlacedResourceX`

A gap record for #523. It lists what the repository establishes about the name
`CreatePlacedResourceX` and what it does not. Evidence classes are the
A/B/D/E/U classes defined in `docs/claims.toml` (A: executed measurement on a
named Apple Silicon host; B: source-derived semantics from a public vendor
specification; D: public documentation or sample-source inspection without
console execution; E: public tool/format documentation or observed behavior of
a public software tool; U: unknown/unestablished by the reviewed public
sources). Qualifications such as "hypothesis" and "unknown" describe the
answer, not a separate evidence vocabulary.

## Where the name appears

The string `ID3D12Device11[44]::CreatePlacedResourceX` occurs on this branch
only inside synthetic JSONL fixtures in Rust unit tests:

- `launcher/crates/bx-telemetry/src/lib.rs:276` — the sample log line.
- `launcher/crates/bx-telemetry/src/lib.rs:290` — the assertion key.
- `launcher/crates/bx-cli/src/main.rs:1589` — the sample log line in
  `a_log_without_shader_gate_records_renders_as_before`.
- `launcher/crates/bx-cli/src/main.rs:1604` — the expected rendered trace
  line in the same test.

**Class A** (local `grep -rn 'CreatePlacedResourceX'` over the working tree on
`track-f/523-placed-resource-x-gap`, rebased onto `origin/main`): no
occurrence exists in `shims/`, `abi/`, `tests/`, `scripts/`, or `docs/` other
than this document. The `bx-cli` fixture arrived on `origin/main` with
`463169f7` (`[M2-A3] bx-cli trace: summarize shader_gate, D3DMetal-gap and
bridge-missing records (#497)`), which is an ancestor of this branch after the
rebase; it is a second synthetic fixture, not a console source.

## What the shim does

**Class A** (local `grep` of `shims/`): `ID3D12Device11` is not named anywhere
in `shims/`. The `d3d12_xs` proxy models `ID3D12Device` through
`ID3D12Device10`, which is as far as the mingw-w64 `<d3d12.h>` declares.

**Class A** (local source inspection):
`shims/d3d12_xs/tools/gen_proxy.py:22` sets `PAD_SLOTS = 32`. For each
generated interface it emits 32 `BxBeyondPublicVtableNN` slots after the
public vtable. Each one calls
`BX_VTABLE_STUB_LOG("<iface>", <slot>u, "beyond_public_vtable")` and returns
`E_NOTIMPL` (`gen_proxy.py:190-194`; `abi/include/bx_log.h:21` defines the
macro). The generated comment states that names and signatures of methods
beyond the public vtable are unknown to the proxy.

**Class A** (local source inspection): the literal method name emitted by
those padded slots is `beyond_public_vtable`. The shim never emits
`CreatePlacedResourceX`.

## What is not known

The Xbox `ID3D12Device11` interface is NDA-gated (`docs/claims.toml`
`CLAIM-XBOX-GDK-ACCESS`, class D). No public source reviewed here defines it.

| Question | Answer | Evidence class |
| --- | --- | --- |
| Does the name `CreatePlacedResourceX` appear in the repo? | Yes, in the synthetic telemetry and `bx-cli` trace fixtures listed above | A |
| Is slot 44 of `ID3D12Device11` named `CreatePlacedResourceX`? | The fixtures assert it; no console source confirms it, so the slot name stays a hypothesis | U |
| What is the console signature of `CreatePlacedResourceX`? | Not defined in `abi/` or any public source reviewed; unknown | U |
| Does the shim implement it? | No | A |
| Does the shim log that name? | No; it logs `beyond_public_vtable` | A |
| Is `ID3D12Device11` proxied at all? | No | A |

## The PC API is a lead, not the contract

`ID3D12Device::CreatePlacedResource` **is** public on Microsoft Learn
(<https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource>,
retrieved 2026-11-27). **Class B** (source-derived from the public D3D12
documentation, scoped to public Windows D3D12) signature:

```cpp
HRESULT CreatePlacedResource(
  ID3D12Heap                *pHeap,
  UINT64                    HeapOffset,
  const D3D12_RESOURCE_DESC *pDesc,
  D3D12_RESOURCE_STATES     InitialState,
  const D3D12_CLEAR_VALUE   *pOptimizedClearValue,
  REFIID                    riid,
  void                      **ppvResource
);
```

Per `AGENTS.md` §5, the Xbox variant must **not** be inferred from this PC
API. The seven-parameter PC signature is cited only as a lead for where to
look; it is not the console contract, and no parameter count, ordering, or
semantics are asserted for `CreatePlacedResourceX`.

## Scope

Documentation only. No shim, ABI, or test change is proposed here. Closing the
gap requires an authorized source for the console interface; until then the
slot name stays a hypothesis (class U) and the signature stays unknown
(class U).
