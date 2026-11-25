# Remill experiment provenance and preliminary license review

This records a local research build, not redistribution clearance. Original
BetterXen fixture/harness code is GPL-3.0-or-later. No upstream emulator source
was copied or adapted into the experiment. Remill code is compiled externally;
the harness includes its external state headers and links generated semantics.
Keep all upstream notices and source with any future redistributed derivative.

| Component | Exact source revision | Reviewed license/notice |
| --- | --- | --- |
| lifting-bits/remill | `66c488c9911fd9b368e4d0c9647d74239f8bba8b` | Apache-2.0, root `LICENSE`; file notices in `include/remill/Arch/X86/Runtime/State.h`, the included runtime headers, `lib/Arch/X86/Semantics/BINARY.cpp`, `DATAXFER.cpp`, `CALL_RET.cpp`, `MISC.cpp`, `MMX.cpp`, `LOGICAL.cpp`, and `CMakeLists.txt`. Trail of Bits copyright notices remain intact. |
| gflags/gflags | `52e94563eba1968783864942fedf6e87e3c611f4` | BSD-3-Clause, `COPYING.txt`. |
| google/glog (v0.7.1) | `7b134a5c82c0c0b5698bb6bf7a835b230c5638e4` | BSD-3-Clause, `COPYING`. |
| google/googletest (v1.17.0) | `52eb8108c5bdec04579160ae17225d66034bd723` | BSD-3-Clause, `LICENSE`; built as part of upstream's dependency superbuild. |
| intelxed/xed (v2025.06.08) | `dc6bdbe036515c7ef55d3b2d50bafc5232bfe534` | Apache-2.0, `LICENSE`. |
| intelxed/mbuild (v2024.11.04) | `7c4497f41f576b43a80bb0f8d8452bbcfd58b6e2` | Apache-2.0, `LICENSE`; build tool. |
| lifting-bits/sleigh | `7c6b7424467d0382a1303c278633e99b0d094d5b` | Apache-2.0, `LICENSE`. |
| NationalSecurityAgency/ghidra | `80ccdadeba79cd42fb0b85796b55952e0f79f323` before upstream Sleigh patches | `LICENSE` and `NOTICE`; sampled decompiler source headers carry Apache-2.0. Generated `xml.cc` also contains the FSF GPL-3.0-or-later Bison skeleton notice and its larger-work exception. Do not describe every Ghidra file as solely Apache-2.0. |
| LLVM/Clang | Homebrew LLVM 17.0.6 | Apache-2.0 WITH LLVM-exception; native external build/lifting tools. |

The Remill checkout is unchanged. Sleigh's superbuild applies its published
Ghidra patches and Remill's `dependencies/sleigh_patches/*.patch`; no experiment
patch was added to these upstream trees. The locally patched Ghidra checkout
initially ended at `9660b9036d4247eed1e572486db3ad1f839b763d`; the full driver
rebuild ended at `6a53bbfaa98fe272d12d7ba38b2e7ef96355e30f`. Patch-generated commit IDs
can depend on the build environment; the source base and patch content are the
reproducibility inputs. Git identities in the BetterXen checkout were not changed.

Apache-2.0 and BSD-3-Clause are compatible in principle with GPL-3.0-or-later;
the Bison notice must also be retained where applicable. This preliminary review
does not clear all transitive source files, generated language specifications,
system SDK terms or future binary packaging. Before adoption/distribution,
audit the actual linked files, preserve required license/copyright/NOTICE text,
record every modification and satisfy GPL corresponding-source requirements.
The bridge's locked Cargo dependencies are separate from the production lockfile;
their redistribution notices also require review for any shipped artifact.

No foreign source is vendored by this task. Machine-specific dependency revisions
are recorded in `artifacts/remill-evaluation/dependency-revisions.json`; original
licenses, notices, patches and source stay in the ignored upstream build trees.
