# Vendored DirectStorage headers (#483)

`dstorage.h` and `dstorageerr.h` are Microsoft's public PC DirectStorage headers, copied **unmodified** (CRLF line endings preserved) so that `XCRDAPI.dll` and `test_xcrdapi` can implement and call the public `IDStorageFactory`, `IDStorageFile`, `IDStorageQueue`…`IDStorageQueue4` and `IDStorageStatusArray` interfaces.

| Item | Value |
| --- | --- |
| Source | NuGet package [`Microsoft.Direct3D.DirectStorage` 1.4.0](https://www.nuget.org/packages/Microsoft.Direct3D.DirectStorage/1.4.0), downloaded from `api.nuget.org` on 2026-11-26 |
| Package SHA-256 | `0838e7f71b14acfbd1ac7ecc5e943f6c95e4f4e679e8a67f8ff6e7c102414321` |
| Files | `native/include/dstorage.h` (SHA-256 `3f966750423b59145bd05b2ccd889b819f0484813baafbc1345cb16e544b4f87`), `native/include/dstorageerr.h` (SHA-256 `07ea8182503c25dd1684d3f9105db7b6fa70fc96dcdab18bd9de75fcec427fa2`), `LICENSE-CODE.txt` (SHA-256 `903df5512f7d02609fed0c780a9b704f5a3eeb6e4d84ebe42a29845c81899a3c`) |
| Licence | MIT. Both headers start with `Copyright (c) Microsoft Corporation` / `Licensed under the MIT license`; the package's `LICENSE-CODE.txt` (copied here) holds the MIT text. |
| Modifications | None |

The package as a whole is under the proprietary *Microsoft DirectStorage SDK* licence terms (`LICENSE.txt`), whose distributable list (`distributable_files.txt`) names only `dstorage.dll` and `dstoragecore.dll`. BetterXen uses neither DLL and copies nothing else from the package.

The Xbox console interfaces (`IDStorageFactoryX` and others, `dstorage_xs.h`) are not included: their IIDs have no licence-compatible public source. See the discussion on #483.
