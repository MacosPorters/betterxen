# CPU boundary research

**Historical native feasibility notes.** These unknowns and proposed evaluations predate the bounded ARM64 translator now preserved in `crates/loader`. They do not describe CrossOver CPU translation or an active native work queue. See the [active architecture](../architecture.md).

Microsoft's March 2020 Xbox Series X hardware disclosure identifies the shipped CPU family as custom AMD Zen 2. The vendor hardware disclosure carries class D (CLAIM-XBOX-SERIES-X-CPU). Using x86-64 as the future guest ISA is based on that family identification. Platform-specific calling conventions and guest virtual-memory rules remain unknown. [Xbox Series X hardware disclosure](https://news.xbox.com/en-us/2020/03/16/xbox-series-x-tech/)

Apple documents the JIT mapping and write-protection APIs for Apple Silicon, including instructions for making newly written machine code visible to instruction fetch. Class B applies to the documented host APIs. At the time of this study, Xenora had not chosen ahead-of-time translation or JIT compilation. The preserved implementation now includes a bounded ARM64 dynamic binary translator; see the [instruction registry](../cpu/instruction-registry.md). [Apple JIT porting guide](https://developer.apple.com/documentation/apple-silicon/porting-just-in-time-compilers-to-apple-silicon)

No guest page-size contract has been established. Host mapping operations use host page granularity. Any finer guest protection granularity will require an explicit software policy. Class B applies to documented host mapping semantics. [Apple mmap manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/mmap.2.html)

Future evaluation includes guest atomic and memory ordering, x87 and SSE numerical behavior, x86 vector mapping, guest scheduling and public system APIs. The graphics contract assumes no host TSO mode.
