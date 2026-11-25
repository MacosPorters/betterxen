# victory-shader in BetterXen

This is the active hybrid shader integration, distinct from the paused native
`xenora-shader` MSL compiler. The pinned upstream snapshot translates GFX10
machine code to PC DXIL. BetterXen's `emit::translate_native` entry point and
fixture tests exercise original compute shaders with explicit root layouts.

Read [UPSTREAM.md](UPSTREAM.md) for attribution, exact source, modifications and
scope, and the [integration report](../../docs/research/victory-shader-integration.md)
for initial failures and the local results after the included wave fixes. Upstream container-parsing APIs are
present, but no authorized console shader container was validated. Their API
documentation describes formats, not BetterXen compatibility guarantees.

```sh
CARGO_BUILD_JOBS=2 cargo +1.100.0 test --locked -p victory-shader
```

On macOS this runs CPU-side tests; Windows-only GPU modules are not compiled.
The [hybrid testing guide](../../docs/testing/README.md) explains cross-building,
external CrossOver requirements and the separate runner dependencies needed for
GPU readback. Preserve skipped-test distinctions and do not generalize original
fixture results to arbitrary console shaders or games.
