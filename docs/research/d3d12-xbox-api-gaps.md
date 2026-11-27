# Xbox D3D12 API gap: `CreatePlacedResourceX`

A gap record for #523. It lists what the repository establishes about the name
`CreatePlacedResourceX` and what it does not. Evidence classes follow
`docs/claims.toml`; the labels below use the expanded names.

## Where the name appears

The string `ID3D12Device11[44]::CreatePlacedResourceX` occurs in this branch
only inside a synthetic JSONL fixture in the telemetry unit test:

- `launcher/crates/bx-telemetry/src/lib.rs:119` — the sample log line.
- `launcher/crates/bx-telemetry/src/lib.rs:133` — the assertion key.

**VERIFIED** (by `grep -rn 'CreatePlacedResourceX'` over the working tree on
`track-f/523-placed-resource-x-gap`): no occurrence exists in `shims/`,
`docs/`, `abi/`, `tests/`, `scripts/`, or `CHANGELOG.md`.

**VERIFIED**: `launcher/crates/bx-cli/src/main.rs` on this branch contains no
occurrence. A `bx-cli` trace fixture carrying the same string exists on commit
`fb47ffd` (`bx-cli trace: summarize shader-gate records`), which is **not** an
ancestor of this branch. That second fixture is therefore a **SOURCE-DERIVED**
lead about another branch, not a fact about this one.

## What the shim does

**VERIFIED**: `ID3D12Device11` is not named anywhere in `shims/`. The
`d3d12_xs` proxy models `ID3D12Device` through `ID3D12Device10`, which is as
far as the mingw-w64 `<d3d12.h>` declares.

**VERIFIED**: `shims/d3d12_xs/tools/gen_proxy.py:22` sets `PAD_SLOTS = 32`.
For each generated interface it emits 32 `BxBeyondPublicVtableNN` slots after
the public vtable. Each one calls
`BX_VTABLE_STUB_LOG("<iface>", <slot>u, "beyond_public_vtable")` and returns
`E_NOTIMPL` (`gen_proxy.py:190-194`; `abi/include/bx_log.h:21` defines the
macro). The generated comment states that names and signatures of methods
beyond the public vtable are unknown to the proxy.

**VERIFIED**: the literal method name emitted by those padded slots is
`beyond_public_vtable`. The shim never emits `CreatePlacedResourceX`.

## What is not known

The Xbox `ID3D12Device11` interface is NDA-gated (`docs/claims.toml`
`CLAIM-XBOX-GDK-ACCESS`, class D). No public source reviewed here defines it.

| Question | Answer | Evidence class |
| --- | --- | --- |
| Does the name `CreatePlacedResourceX` appear in the repo? | Yes, in one synthetic telemetry fixture on this branch | VERIFIED |
| Is slot 44 of `ID3D12Device11` named `CreatePlacedResourceX`? | The fixture asserts it; no console source confirms it | HYPOTHESIS |
| What is the console signature of `CreatePlacedResourceX`? | Not defined in `abi/` or any public source reviewed | UNKNOWN |
| Does the shim implement it? | No | VERIFIED |
| Does the shim log that name? | No; it logs `beyond_public_vtable` | VERIFIED |
| Is `ID3D12Device11` proxied at all? | No | VERIFIED |

## The PC API is a lead, not the contract

`ID3D12Device::CreatePlacedResource` **is** public on Microsoft Learn
(<https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource>,
retrieved 2026-11-27). **SOURCE-DERIVED** signature:

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
slot name stays a hypothesis and the signature stays unknown.
