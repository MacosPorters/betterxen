# Third-party notices

| Component | Version | License | Origin and notice |
|---|---|---|---|
| AMD RDNA 2 machine-readable ISA | 2026-02-20, schema 1.1.1 | MIT | Generated opcode masks derive from the AMD source identified in the [RDNA 2 research notes](docs/research/rdna2.md). [AMD license and copyright](crates/isa/LICENSE-AMD). |
| bitflags | 2.13.2 | MIT selected | [Upstream revision](https://github.com/bitflags/bitflags/tree/80ce9b545acb0bd42150695fc351889cac1d8eb4), [MIT license and copyright](licenses/bitflags-LICENSE-MIT). |
| objc2 | 0.6.5 | MIT | [Upstream revision](https://github.com/madsmtm/objc2/tree/d7d2fa23ceaa5e6096c923b081040e5d81b3b9df), [upstream licensing notice](licenses/objc2-LICENSE.md). |
| objc2-encode | 4.1.0 | MIT | [Upstream revision](https://github.com/madsmtm/objc2/tree/8d214f5477365ffcbcbb7de058c86ed9a518efb7), [upstream licensing notice](licenses/objc2-LICENSE.md). |
| objc2-foundation | 0.3.2 | MIT | [Upstream revision](https://github.com/madsmtm/objc2/tree/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b), [upstream licensing notice](licenses/objc2-LICENSE.md). |
| objc2-core-foundation | 0.3.2 | MIT selected | [Upstream revision](https://github.com/madsmtm/objc2/tree/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b), [upstream licensing notice](licenses/objc2-LICENSE.md). |
| objc2-metal | 0.3.2 | MIT selected | [Upstream revision](https://github.com/madsmtm/objc2/tree/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b), [upstream licensing notice](licenses/objc2-LICENSE.md). |

Cargo resolves the Rust packages from crates.io at the source checksums in [Cargo.lock](Cargo.lock). The upstream objc2 notice discusses the derivation of bindings from Apple SDKs and an open question under Apple’s SDK terms. The maintainer confirmed upstream’s published license position in [issue #895](https://github.com/madsmtm/objc2/issues/895). No Apple SDK files or dependency crate archives ship in this repository. The [upstream MIT text](licenses/objc2-LICENSE-MIT.txt) is an unmodified copy from [objc2 commit ee9a7ad](https://github.com/madsmtm/objc2/blob/ee9a7ada2131f5944b8750428e265c15632f2a19/LICENSE-MIT.txt), added after the locked source revisions. Preserve it and the pinned licensing notice. See the [G11 review](docs/licenses.md#runtime-redistribution-review) for unresolved attribution and SDK questions.

[LLVM 23.1.6](https://github.com/llvm/llvm-project/tree/5b10d2d74929eae78912771222c9855af97496f4) is used as an external fixture tool under Apache-2.0 WITH LLVM-exception. Xcode and Metal are separate Apple tools under Apple's license terms.

The [isolated fuzz workspace](fuzz/Cargo.lock) resolves developer-only dependencies, including [libfuzzer-sys](https://github.com/rust-fuzz/libfuzzer/tree/719e4efb9b8857ebaa782ae59376c8cbb78fed0f). See the [fuzz dependency inventory](docs/licenses.md#fuzz-development-dependencies) for locked revisions and selected licenses.

llvm-mingw release 20261117 is used as an external build tool for original Windows test programs under Apache-2.0 with LLVM exceptions plus its component licences; nothing from it is shipped or copied (see [hybrid-toolchain](docs/research/hybrid-toolchain.md)). `crates/hybrid/src/memory.rs` is a Rust reimplementation of the memory-guard logic in skakwy's [victory-mac](https://github.com/skakwy/victory-mac) `mac/run-victory.sh` (commit 22b3f8e45a59, GPL-3.0), used with the author's permission.
