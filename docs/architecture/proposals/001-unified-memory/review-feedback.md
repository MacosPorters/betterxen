# Review feedback preserved for the pre-M3 discussion

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

[`devil8blade`'s comment review](https://github.com/MacosPorters/betterxen/pull/495#pullrequestreview-6082941653) of `9f93979`, 2026-11-27. The reviewer ran nothing and saw a truncated diff. Paraphrased below; the original governs. **None of it is applied to the design yet.**

| # | Feedback | Author's first view |
| --- | --- | --- |
| 1 | Define eligibility as safe, not complete: require positive proof, otherwise fall back. A false positive breaks pointer stability. Candidate criteria include buffers only; no UAV, render-target or depth use; CPU-visible; no requested address, split commit, scatter view or DirectStorage destination | Agree with the principle |
| 2 | The five-argument question deserves its own Track B issue; a clean-room synthetic test can check the call shape | Maintainers' call; no issue created |
| 3 | Admission should be two-phase with cancel, share `xmem`'s lock, cover every creation path, charge rounded host size, and count a byte once. Keep the shadow-commit idea to a short experiment | Agree |
| 4 | Add a startup check of the release behaviour that falls back to observe-only; the callback should only enqueue; say whether the cap counts live or retained bytes | Agree; not designed |
| 5 | A pointer given to the guest must stay valid until `XMemFree`, surviving reconfigure and device loss, or be refused. Make it a gate | Agree; #486's stale pointer is the example |
| 6 | Keep to Track F files; drop the changelog line; avoid committing fast-moving PR heads; give each stage an owner and merge order (ABI, `xmem`, `d3d12_xs`) | Changelog is required by project rules and was shortened. Owners wait for the discussion |
| 7 | Numeric go or no-go gates; fixtures that fail without the mechanism; move probes into `tests/conformance/` | Agree; no code here |
| 8 | The document set is too large; a short decision record plus appendix would be easier | Done: three short pages and an appendix |

## Related reviews held elsewhere

- Research publication: `devil8blade`'s review on [discussion 493](https://github.com/MacosPorters/betterxen/discussions/493), including a process-listing redaction request.
- #486 follow-up list: the reviewer's comment on [PR #486](https://github.com/MacosPorters/betterxen/pull/486).
