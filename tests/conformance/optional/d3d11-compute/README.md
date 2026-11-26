# D3D11 compute-buffer workload

This original fixture submits an unsigned 32-bit addition in a Direct3D 11 compute shader and verifies the GPU output through CPU staging readback. It requires a hardware device at feature level 11.0 and uses no game data.

## Expected output

The shader adds one plus the lane's position within a repeating 16-word sequence. Write an output index as `16b + r` with `0 ≤ r < 16`. Pattern 0 starts its block at `4096 + 65536b`. Pattern 1 starts at `−16 − 65536b`, represented modulo 2³². Each input is its block's base plus `r`. The expected output is the corresponding odd-number progression from base plus 1 to base plus 31, reduced modulo 2³². The C code lists both first-block progressions as constants. Subsequent blocks shift the expected progression by the signed block offset. The block tags distinguish writes whose indices differ by a multiple of 16. The second pattern exercises unsigned wraparound in its first block.

Each pattern runs with counts 0, 1, 63, 64, 65, 127, 128, 129, and 191. A zero count skips dispatch and checks that the entire buffer retains its initial sentinel. The other cases dispatch groups of 64. The structured buffers physically hold 192 words, so an erroneous write from a rounded-up tail would overwrite a sentinel inside the allocated buffer. Every run compares all 192 words, including undispatched slots. The negative control changes only the expected word for pattern 0, count 65, index 64 from `00041001` to `00041000`.

The digest is 32-bit FNV-1a over all 3,456 readback words, each fed as four bytes in little-endian order. Pattern 0 comes first, each pattern uses the count order above, and each buffer is traversed by ascending index. The analytic result is `6ce404b5`. The negative control should give exactly one mismatch with the same digest because it leaves the GPU inputs and work unchanged.

The executable returns 0 for a matching readback, 1 for mismatches, 2 for no feature-level-11.0 hardware device, 3 for another API failure, and 4 for invalid arguments.

## Build and run

Use the external llvm-mingw 20261117 UCRT macOS universal release pinned by the candidate in #384. Its archive SHA-256 is `98d340bbb8d46b09ad6886ffba88bd9d5415e131c316de48e6bd7647377446aa`. Clang and LLD are 23.1.6. The package ships both host slices and the recipe explicitly runs Clang as ARM64. The release includes LLVM's Apache-2.0 license with LLVM exceptions and dependency notices. See the archive's `LICENSE.TXT`. It is a build dependency and is supplied separately.

```sh
tests/conformance/optional/d3d11-compute/build.sh /absolute/path/to/llvm-mingw /absolute/path/to/vkd3d-compiler
```

The second dependency is WineHQ vkd3d-compiler 2.2 with libvkd3d-shader 2.2 (LGPL-2.1-or-later). The WineHQ 2.2 source archive used locally had SHA-256 `6f8cdbfc52e36eb74ab6aff42b1e958c3d9af912ab8f23d16d63711f2bab051b`. Use a native ARM64 build. The recipe compiles HLSL entry `main` to the `cs_5_0` profile in a tokenized DXBC container and embeds the resulting bytes in the executable. The guest gives those bytes directly to `CreateComputeShader`. Neither compiler is bundled or invoked by the guest.

Local compiler build inputs were the WineHQ release, Xcode Command Line Tools, pkgconf 2.5.1, Bison 3.8.2, system Flex, Vulkan-Headers `d4a196d8c84e032d27f999adcea3075517c1c97f` and SPIRV-Headers `3f17b2af6784bfa2c5aa5dbb8e0e74a607dd8b3b`. From the extracted vkd3d source, the local ARM64 build used:

```sh
PKG_CONFIG=/absolute/path/to/pkgconf SONAME_LIBVULKAN=libMoltenVK.dylib \
  CPPFLAGS="-I$VULKAN_HEADERS/include -I$SPIRV_HEADERS/include" \
  ./configure --disable-tests --disable-demos
for grammar in hlsl preproc; do
  bison -d -o "libs/vkd3d-shader/$grammar.tab.c" "libs/vkd3d-shader/$grammar.y"
done
make include/private/vkd3d_version.h
make -j2 vkd3d-compiler
```

The Vulkan soname satisfies configure's runtime-library setting for the broader vkd3d project. The compiler target links libvkd3d-shader and does not link Vulkan or MoltenVK. The default guest build output is `artifacts/hybrid-workloads/x86_64/d3d11-compute.exe`.

Use a fresh dedicated Windows 11 64-bit bottle and the evidence-saving runner from #398. The runner's backend selector is observed CrossOver behavior. Record the backend evidence from each run.

```sh
bx-cli run --bottle betterxen-issue-401 --backend dxvk \
  --timeout 45 --expect 'PASS cases=' --out artifacts/issue-401/run-records \
  -- "$PWD/artifacts/hybrid-workloads/x86_64/d3d11-compute.exe"
bx-cli run --bottle betterxen-issue-401 --backend dxvk \
  --timeout 45 --expect 'PASS cases=' --out artifacts/issue-401/run-records \
  -- "$PWD/artifacts/hybrid-workloads/x86_64/d3d11-compute.exe" --negative-control
```

The second command should return 1 and report one mismatch. A result without a guest exit code or completed readback is unavailable.

## Local evidence

Workload branch base: `566272082a74d8b45fcec37edd00f4f6703df3ab` (#384). The separate Rust 1.100.0 runner checkout used `e1b6a24a4f37360abb1f5c77bedbfd470cf1c720` (#398's persistence fix on #384). Host was an ARM64 Apple M2 Pro with 16 GiB and macOS 27.3 (26C5035r). CrossOver Preview was 20261111 (27.0.0.41303). The pinned archive checksum matched. Separate source and output directories produced identical DXBC (SHA-256 `af046791dce24119b0452d50cd30f69a2735de649161101cb5d0626625eebee0`) and x86-64 PE32+ executables (SHA-256 `2f92cd68266592039cdc4860a2c3896a15ddabb83dbecd0d1ed928599009095f`).

The default memory guard refused the first request at 49% free. Subsequent local runs used the runner's `--min-start-free 45` option with the default during-run floor. Run records are under ignored `artifacts/issue-401/precompiled-records/`. This is author-side verification. Independent derivation and CrossOver reproduction are pending.

| Requested backend | Evidence in the run record | Normal run | Negative control |
| --- | --- | --- | --- |
| D3DMetal | Inferred from the `AMD Compatibility Mode` adapter | Exit 0, zero mismatches | Exit 1, one mismatch |
| DXVK | DXVK banner and `Apple M2 Pro` adapter | Exit 0, zero mismatches | Exit 1, one mismatch |

All four completed runs read back 3,456 words and matched the independently calculated digest `6ce404b5`. The initial memory refusal was the only unavailable run in the final evidence set. The runner intentionally classifies a negative-control exit 1 as a workload failure.

## References

- [llvm-mingw 20261117 release](https://github.com/mstorsjo/llvm-mingw/releases/tag/20261117)
- [WineHQ vkd3d 2.2 release](https://list.winehq.org/hyperkitty/list/wine-announce%40list.winehq.org/thread/QBVNHPS2PLCJXA67ESIEY3XZP7BV4KRA/)
- [Microsoft compute shader creation and profile example](https://learn.microsoft.com/en-us/windows/win32/direct3d11/direct3d-11-advanced-stages-compute-create)
- [Microsoft structured-buffer resource](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-object-structuredbuffer)
- [Microsoft thread-group size and dispatch IDs](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/sm5-attributes-numthreads)
- [Microsoft CopyResource reference](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-copyresource)
- [Microsoft Map reference](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-map)
