# D3D11 texture-sampling workload

Original Windows program. It draws a textured triangle with a point sampler into an 8×8 offscreen render target, reads the pixels back, and compares them with a CPU reference. The reference applies the D3D 11.3 point-sample rule (functional spec sections 7.18.7 and 7.18.9) to the texture bytes. It does not read the GPU output. Spec section 19.1.2 requires an 8-bit UNORM point sample, written with blending off, to come back as the same texel.

Two synthetic patterns are checked, `lattice` and `diagonal`. Neither is taken from a game, a capture, or a shader dump. The clear color is magenta, which is not a texel in either pattern, so a pixel the triangle misses cannot pass.

A run that matches prints `d3d11-texture: PASS` and exits 0. `--negative-control` adds one texel to U in the pixel shader and leaves the reference unbiased. The CPU model predicts that this matches only the last column (8 of 64 pixels) and therefore fails. That run prints `d3d11-texture: FAIL` and exits 1. If the bias is ignored and every pixel still matches, the program also prints FAIL. One CrossOver run of both commands is recorded under Run.

The result digest is FNV-1a 64 over the two readbacks in pattern order, printed as 16 hex digits on the `digest=` line. Each pattern line also has its own digest.

Exit 2 is a bad argument. Exit 3 means the device, shader, or readback could not be set up. Device creation tries a hardware driver, then WARP. There is no window.

## Build

The program targets 64-bit Windows. The link line passes `-ld3d11 -ldxgi -ld3dcompiler -lole32`. The linked PE32+ console binary imports `d3d11.dll` (`D3D11CreateDevice`) and `D3DCOMPILER_47.dll` (`D3DCompile`). `sample.hlsl` is embedded and compiled at runtime with `D3DCompile`, so an HLSL compiler is not required on the build machine. `D3DCOMPILER_47.dll` is required where the program runs.

From this directory, with mingw-w64:

```sh
make
```

`make` uses `x86_64-w64-mingw32-gcc` unless `CC_WIN` is set:

```sh
make CC_WIN='clang --target=x86_64-w64-mingw32'
```

The same compile, once `sample_hlsl.h` has been generated:

```sh
python3 embed_hlsl.py sample.hlsl sample_hlsl.h
x86_64-w64-mingw32-gcc -std=c11 -O2 -Wall -Wextra -Werror -mconsole -I. \
    -o d3d11-texture.exe main.c texture_ref.c \
    -ld3d11 -ldxgi -ld3dcompiler -lole32
```

The Windows sources are `main.c` and `texture_ref.c`. `host/ref_selftest.c` is a host check of the CPU reference only. It does not create a device:

```sh
make check-reference
```

## Run

On Windows, or under CrossOver on a Mac:

```sh
d3d11-texture.exe
d3d11-texture.exe --negative-control
```

devil8blade ran both commands on one Apple Silicon Mac. The directory was built there with x86_64-w64-mingw32-gcc 16.2.0 (`make` exited 0, `-Werror` clean) and `make check-reference` with the macOS `cc` printed `texture-ref: PASS`. The bottle was a fresh win10_64 bottle in CrossOver 26.2.0, using CrossOver's built-in `d3d11.dll` and `d3dcompiler_47.dll` (wined3d, not DXVK or DXMT).

The plain run exited 0:

```
d3d11-texture: device=hardware feature_level=11_0
d3d11-texture: pattern=lattice matched=64/64 digest=0fc3b125a4713125
d3d11-texture: pattern=diagonal matched=64/64 digest=3e1da1d5054c4a65
d3d11-texture: digest=68e7d02e7fd35865
d3d11-texture: PASS
```

`--negative-control` exited 1. Lattice matched 8/64 (`digest=3c453f6c404f8a25`), diagonal matched 8/64 (`digest=b5e1b11c383e3f85`), the combined digest was `d8fe45ff1907a685`, and the last line was `FAIL`. Each actual pixel was the next texel to the right.

That is this bottle on this Mac only.
