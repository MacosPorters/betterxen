# Open Phase 01 risks

**Historical native Phase 01 register.** Owners and next experiments below retain their original candidate scope; they are not current assignments or hybrid priorities. Active hybrid risks and exit conditions are in the [roadmap](roadmap.md), with component boundaries in [architecture.md](architecture.md).

| ID | Risk and impact | Current evidence | Owner and next experiment |
|---|---|---|---|
| R1 | Console shader container, launch state and resource descriptors remain unknown, so the fixture cannot establish console compatibility. | Public related-target specifications, class C | Xbox research. Seek an authorized exact-target specification before proposing a parser. |
| R2 | FP32 subnormal inputs can affect normal outputs. The historic V4 input-only probe reported four signed-zero mismatches. | Historical host observation, pending project reproduction | Metal and testing. Reproduce the original strict known answers in a bounded native Rust test. |
| R3 | FP32 subnormal outputs can be flushed even with normal inputs. The historic V4 output-only probe reported three signed-zero mismatches. | Historical host observation, pending project reproduction | Metal and testing. Reproduce both signs and the boundary rounding case. |
| R4 | The V3.1 exceptional arithmetic probe reported five subnormal mismatches and a canonical NaN payload. | Historical host observation, pending project reproduction | Compiler and reference. Keep the strict historical comparisons and establish the required guest policy before supporting float lowering. |
| R5 | Redistribution rights for SDK-derived Rust bindings and locked-version attribution require review before distributing linked binaries. | Pinned upstream license notices and maintainer comments | R1. Review the actual linked artifact and applicable terms. |
| R6 | Compiler and reference previously disagreed on skipped early termination and invalid buffer bindings. | R2 analysis and compiler regression tests | Both now check layout and buffer bindings across the stream and execution state on the selected path. Independent candidate review remains open. |
