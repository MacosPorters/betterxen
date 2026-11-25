# Wave-semantics HLSL probes

Original BetterXen sources, GPL-3.0-or-later. They are compiled with Microsoft's open-source DXC
(`dxc -T cs_6_6 -E main`; release v1.9.2609, `dxc_2026_09_29.zip`, SHA-256 `ad31b1fc...a7e7f1`, equal to
the release digest; an external tool, not vendored) and dispatched with `d3d12_run.exe run <shader.dxil> 1 <dwords>`
under CrossOver. Expected values were worked out by hand before running. Results and what they show:
`docs/research/shader-wave-semantics.md`.

```sh
wine --bottle <bottle> ./dxc.exe -T cs_6_6 -E main -Fo sum_f64_4.dxil sum_f64_4.hlsl   # dxcompiler.dll and dxil.dll beside it
wine --bottle <bottle> ./d3d12_run.exe run sum_f64_4.dxil 1 8
```
