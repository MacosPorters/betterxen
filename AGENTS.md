# BetterXen engineering rules

These rules apply to every contributor and coding agent. They were rewritten on the `planning/hybrid-first-transition` branch when BetterXen moved from native-first to CrossOver-based hybrid development. `CLAUDE.md` is a symlink to this file. The decision is recorded in `docs/adr/0005-crossover-hybrid-direction.md`; the plan is `docs/roadmap.md`.

### Direction

- BetterXen is hybrid-first Apple Silicon compatibility research. **CrossOver is the primary compatibility foundation and hybrid is the only active development direction.** CrossOver is an externally installed dependency unless its license explicitly permits another distribution model.
- BetterXen's own work is the layer around it: integration, configuration, environment management, diagnostics, reproducible testing, benchmarking and measured optimization. Reuse established infrastructure before writing an independent replacement; write one only for a demonstrated limitation.
- The independent native implementation (x86-64 decoder, ARM64 translator, runtime, gfx1030 shader compiler, Metal backend) is **preserved and paused**. Do not delete it, expand its roadmap, create speculative native issues or assign contributors to it by default. Native work is allowed only when hybrid development directly requires it, or after an explicit maintainer decision to resume (`docs/research/native-roadmap.md`). Keep its tests passing.
- This is the intended architecture, not a verified Xbox compatibility stack. Do not assume CrossOver, Rosetta 2 or D3DMetal provide Xbox runtime compatibility; verify the actual capabilities and integration boundaries of the installed versions.

### Planning

- Planning is progressive. Only the current phase has a milestone and open issues; the roadmap outlines later phases provisionally and `Parked ideas` holds future items. Do not create speculative issues, trackers or milestones for distant phases; promote an idea when its prerequisites are met.
- Every new issue states a specific problem, the evidence it is needed, a deliverable, upstream references, a reuse assessment, dependencies, acceptance criteria and a validation method. Do not create administrative issues to manage planning.
- Do not open native-only issues. If hybrid work needs a native component, file a focused hybrid issue.

### Dependencies and reuse

- Wine-based compatibility infrastructure, CrossOver, Rosetta 2 where supported and technically appropriate, and D3DMetal (subject to its license terms) are permitted. DXMT, DXVK, VKD3D-Proton, MoltenVK, FEX and similar projects may be evaluated and used where appropriate. Legitimate external dependencies and open-source implementations are encouraged.
- Before integrating or copying anything: verify the exact license of the component (the file for copied source, not just the repository), check compatibility with GPL-3.0-or-later and with every other selected component, preserve copyright notices, record the source repository and exact commit or version, document modifications, and add the entry to `docs/research/upstream-registry.md` and `docs/research/component-reuse-matrix.md`. If licensing is uncertain, do not use it until resolved. Never remove upstream notices.
- Do not bundle, modify or redistribute proprietary components (CrossOver binaries, D3DMetal, Rosetta 2, Apple tools) unless their license explicitly allows it. Do not assume a proprietary component can be incorporated into GPL-licensed source. Distinguish open-source parts from proprietary parts in every assessment.
- Do not bypass licensing or activation requirements of any dependency.
- Provenance first: exclude anything derived from leaked proprietary material or unauthorized software. A project's license being GPL does not clear material it was built from or for. Projects whose stated purpose depends on leaked or unauthorized builds, including VIctory, its forks and similar efforts, are discovery references at documentation level; adopt nothing from them without a per-file provenance and license review recorded in the registry. Game-specific, asset-specific and decryption-related code is never adopted.

### Native Apple Silicon host

- BetterXen's own host executables, libraries and tools must support ARM64 natively; verify with file, lipo -archs and otool -L. Do not add x86-64 host executables that BetterXen itself builds or ships.
- Rosetta 2 may be used where Apple supports it and CrossOver or another selected component relies on it for x86-64 guest code. Record versions and its support status; do not depend on it for BetterXen's own code.
- Support M1 through M5 and future generations through runtime capability detection. Do not assume all Apple GPUs share capabilities.
- Use aarch64-apple-darwin as the Rust target. Metal is the host graphics API. Do not add graphics abstraction layers beyond what a selected component or a measured need justifies.
- SPIR-V remains permitted as a compiler intermediate representation.

### Honesty and evidence

- Never report a test as passed unless it ran and passed. Never count a skipped test as GPU verification.
- Never invent benchmark results, hardware compatibility, API names, instruction encodings, hardware capabilities, interface details, product capabilities or implementation status. Never promise a performance improvement before measuring it.
- Never claim a clean build unless the commands actually succeeded.
- Never present planned features as working. State clearly that game execution is not supported unless demonstrated.
- Never replace GPU tests with mocks and then claim GPU support.
- Unknown information is classified as unknown and becomes a documented research task. Facts about CrossOver, Rosetta 2, D3DMetal and other upstreams must cite vendor or upstream documentation, or a recorded observation of a stated version.
- Every hardware claim carries an evidence class (see docs/claims.toml). Never present a lower-class claim as if it were a higher-class one.
- Never claim "verified on Xbox hardware". BetterXen has no Xbox hardware access. Agreement with a spec-derived reference is not agreement with Xbox hardware, and docs must say so.
- Do not hide errors behind fallbacks. Do not translate unsupported instructions into silent no-ops. Do not weaken or delete tests to make them pass. Do not change mutation counting, tolerances or gate definitions to improve a score.
- A phase is complete only with the evidence its exit conditions name. Do not derive progress from commits, lines of code, closed issues or isolated passing fixtures.

### Independence of verification

- The agent that writes a component does not certify it. Independent reviewers re-derive expectations from primary sources and are not told the implementers' conclusions as ground truth.
- The same faulty code must never produce both the expected and the actual values in a test.

### Legal and clean-room

- Use only publicly accessible documentation, publicly licensed software and authorized materials. All validation uses independently authored or authorized workloads. Never obtain or use leaked executables or SDKs, confidential Xbox documentation, restricted console tools, firmware, encryption keys, unauthorized assets or source code, or proprietary game builds and shader collections. Do not attempt to bypass documentation access restrictions.
- Contributors who have seen leaked or confidential Xbox materials must not contribute. State this in CONTRIBUTING.md and require a DCO sign-off (Signed-off-by) on contributions.
- docs/legal.md states: BetterXen does not circumvent console security or DRM, does not load encrypted or protected content, and is a research project built from public documentation. This is not legal advice; contributors should know their local laws, including anti-circumvention provisions such as DMCA section 1201.
- Keep BetterXen's own implementation original where it is original. Never copy, port, translate or adapt source from emulator projects built for leaked or unauthorized material (for example PE-Vulkan, KyTx, AnyPS5 and similar).
- Validate the technical compatibility and safety of user-provided files. Do not ask for ownership receipts, accounts or acquisition provenance as part of file loading. Contributor provenance and licensing obligations still apply.
- Never place restricted material in the repository or its Git history.
- Do not push, publish, or change remote visibility without explicit user authorization. Prepare locally and report readiness.

### Quality

No AI slop (section 29).

Read docs/roadmap.md, docs/interfaces.md and docs/progress.md before implementation.

## Claiming work before you start

Several people and coding agents work on this repository at the same time. Claim an issue before you edit anything, and never take work someone else has claimed.

1. **Check.** Read every comment on the issue and look for open pull requests that link it. A claim is any comment in which someone says they are taking the issue, in any words (for example "I'll take this" or `/claim`). If a claim exists and is not stale (step 4), pick another issue.
2. **Claim.** Reply on the issue in plain words, for example: "I'm taking this. Plan: …". No command, label or bot step is needed. The earliest claim wins. Claim one issue per comment.
3. **Stay in scope.** If the issue lists files, edit only those, and ask on the issue before widening the list. Do not start work that edits the same files as another claimed issue.
4. **Stale claims.** A claim is stale when the claimer has not opened a pull request for the issue within about one week (7 days) of the claim. Only then may someone else take it, by replying on the issue, for example: "Taking this over: @name's claim from <date> has no pull request after 7 days." A claim with an open pull request is never stale; ask the claimer or the maintainer instead.
5. **Release.** If you stop working on an issue, say so on the issue so others can take it.
6. **Link.** Put `Closes #<n>` in the pull request description.

A coding agent claims in the name of the person it works for and follows the same rules. It must read the comments before claiming, and must never claim over, remove or ignore another person's claim. The optional task-queue commands in `docs/coordination/agent-tasks.md` do not replace these rules: a plain-reply claim counts even though the bot does not track it.

## Changelog maintenance

- At the start of every engineering task, read `CHANGELOG.md` alongside the relevant project documentation. Review the existing `[Unreleased]` entries before implementation to avoid duplicating completed work.
- Update the changelog in the same task for every meaningful addition, modification, fix, removal or security change, including work implemented by another agent. Use `Added`, `Changed`, `Fixed`, `Removed` or `Security` as appropriate.
- Write concise entries that explain the effect for users or contributors. Distinguish implemented behavior, local verification and independent acceptance. Never invent release dates, versions, benchmark results, passing tests or compatibility claims.
- Omit trivial formatting and intermediate steps unless they materially affect maintainability. Keep unreleased changes under `[Unreleased]` until a release is authorized.
- Before concluding the task, check whether the changelog needs an update and make it when warranted. Rewrite historical entries only to correct factual errors. Preserve existing content when tightening the prose.

## Public documentation and terminology

- Write public documentation so readers can understand it without earlier agent conversations. Keep `README.md` and `docs/README.md` accessible to new contributors.
- Introduce descriptive names before internal identifiers such as X1, G10 or P2-04. Define specialist abbreviations on first meaningful use.
- Write changelog entries around actual behavior and its effect on users or contributors. Distinguish implementation, synthetic demonstrations, independent verification and real Xbox execution.
- Review terminology whenever editing documentation. Use concise, precise explanations and preserve evidence scope.
- Update `docs/README.md` whenever you add, remove, rename or substantially reorganize documentation.

## Subagent reasoning

**Every spawned subagent must use LOW reasoning effort.** Explicitly select LOW and verify the resolved configuration. Never use Medium, High or Extra High. If LOW cannot be guaranteed, proceed without that subagent. The lead agent may use the user-selected reasoning setting.

## Checkout commands

- Format: `cargo fmt --all -- --check`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Test: `cargo test --workspace`
- Verify the synthetic fixtures: `scripts/assemble-fixture.sh --check`
- Run native GPU correctness (preserved native research): `scripts/test-gpu.sh`
- Run the three RDNA-to-Metal fixtures: `scripts/demo.sh`
- Run independent CPU validation: `cargo test --locked -p xenora-semantic-review`
- Run the correctness-gated benchmark suite: `scripts/benchmark.sh` (idle machine, release, validation off)
- Run source mutation on decoder/shader/ref: `scripts/mutation.sh` (R2 owns triage)
- Check generated status tables: `python3 scripts/audit/status.py --check`.
- Check claims and dependency direction: `scripts/audit/claims.sh` and `scripts/audit/dependencies.sh`
- Fuzz one target at a time: `scripts/fuzz.sh <instruction_stream|fixture|reference|runtime_services>` (600 seconds by default). `runtime_services` drives the standard guest service registry.

Keep generated and gate logs under ignored `artifacts/`. Use at most four Cargo build jobs across two simultaneous CPU-heavy jobs, with one GPU job and one fuzz or mutation campaign at a time.

Do not add AI attribution or Co-authored-by trailers to commits.
Never commit or push without explicit permission.
Preserve the configured Git author and committer identity.

## Ownership

These are responsibility areas, not personal assignments; do not assign people without their agreement. Hybrid integration owns the CrossOver assessment, integration boundary, configuration, diagnostics and benchmarks. Xbox and claims owns public console research. ISA owns the decoder and oracle fixtures. Compiler owns the parser, IR and MSL. Metal owns the native GPU backend. Testing owns the independent reference and fixture validation. Infrastructure owns shared Cargo configuration, policies and the CPU boundary. R1 independently audits claims and release evidence. R2 independently validates semantic expectations and mutation results.
