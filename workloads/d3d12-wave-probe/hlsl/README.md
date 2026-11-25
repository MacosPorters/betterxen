# Wave-semantics HLSL probes (NOT YET COMPILED OR RUN)

Original BetterXen sources, GPL-3.0-or-later. They are meant to be compiled with Microsoft's
open-source DXC (`dxc -T cs_6_6 -E main`) and dispatched with `d3d12_run.exe run`. Neither step has
been done: running the downloaded DXC under CrossOver was not authorized in the session that wrote
them, and `d3d12_run.exe run` has only been compiled, not executed (`caps` has been run).
Expected values are worked out by hand from the HLSL specification and listed in each file.
