# Phase 01 mutation testing

**Preserved native research.** This page governs existing synthetic fixtures and their regression checks. It does not define CrossOver compatibility or authorize native expansion; follow the [hybrid architecture](../architecture.md) for active work.

The source campaign covers the decoder, fixture parser,
verifier, lowering and reference interpreter through the default CPU workspace
tests. The independent semantic and oracle campaigns are separate requirements.

## Historical source campaign

The raw outcomes and mutation diffs are under the ignored
`artifacts/mutation/campaign-20261122/mutants.out/`. The campaign used the
source at `a34dadc` with workspace tests, two workers and a 300-second timeout.

| Crate | Generated | Caught | Missed | Unviable | Timeout | Conservative score before equivalence review |
|---|---:|---:|---:|---:|---:|---:|
| Decoder | 116 | 93 | 10 | 12 | 1 | 89.4% |
| Reference | 171 | 139 | 27 | 5 | 0 | 83.7% |
| Shader | 100 | 63 | 29 | 8 | 0 | 68.5% |
| Total | 387 | 295 | 66 | 25 | 1 | 81.5% |

The decoder timeout changed `offset += byte_len` to multiplication. The
offset starts at zero and remains zero after each successful decode. The
historical test phase timed out after 300 seconds following a successful build.
It stays in the denominator as a timeout.

## Reproduction

The targeted rerun matches the exact identities in `missed.txt` and
`timeout.txt`. The full command and outputs are in
`artifacts/mutation/phase01-survivor-rerun/`. Its 30-second timeout distinguishes
the known non-progress case without waiting another five minutes.

Run the full original scope with a fresh output directory after the targeted
rerun and review.

```sh
CARGO_BUILD_JOBS=2 scripts/mutation.sh artifacts/mutation/final-source
```

Count kills over all eligible non-equivalent mutants, including timeouts and
unexecuted mutants. Record invalid mutants and independently reviewed
equivalences separately. A source score cannot substitute for the native
semantic campaign or the validation-fixture oracle campaign.

## R2 review of the historical survivors

Locations refer to each crate's `src/lib.rs` at `a34dadc`. The checked
identities and original outcomes remain in `outcomes.json`. R2 identified
53 genuine test gaps, 13 equivalent survivors and the separate timeout.

| Crate and locations | Proof of equivalence |
|---|---|
| Decoder 352:17 | The descriptor base is a multiple of four. Equality with 102 is unreachable, so `> 102` and `>= 102` are identical. |
| Reference 192:40, 193:42, 197:42, 203:40, 204:47 | OR and XOR pack disjoint bit fields. At each site the shifted contribution occupies bits cleared in the unshifted field. |
| Shader 127:44, 128:42, 137:43 | A bit cannot be known both zero and one. The OR operands are disjoint in each case, including the two ways to establish an XOR output bit. |
| Shader 141:37 | Unknown bits complement the union of known-zero and known-one bits, so they share no bits with the known-one mask. |
| Shader 183:9 | For a singleton result the masks already fully specify its value. The fallback reconstructs the same two identical candidates that the deleted arm would store explicitly. |
| Shader 275:13, 279:13 | The removed arm and weakened predicate affect private defense-in-depth. The sole caller supplies decoder output, which independently rejects backward branches and targets outside instruction boundaries before calling the verifier. |

R2 identified diagnostic, scalar and vector selector, fixture limit, transfer
ordering, reserved register and exact FP32 verifier regressions for the
remaining survivors. The targeted rerun records which tests actually
distinguish each mutant.

## Complete source campaign at aff46b0

Run from the clean candidate on 2026-11-22 with native ARM64 cargo-mutants
27.1.0 and two workers.

```sh
CARGO_BUILD_JOBS=2 scripts/mutation.sh artifacts/mutation/source-aff46b0
```

The 399-entry inventory and finished outcomes contain the same identities.
The baseline passed. Cargo-mutants exited 3 after reporting 360 caught,
13 missed, 25 unviable, and one timeout. Independent R2 review inspected
every survivor, every unviable compilation and each raw caught log.
The conservative count credits named behavioral test failures only.

| Module | Generated | Raw caught | Verified kills | Raw misses | Equivalent | Invalid | Timeout | Other non-progress | Non-equivalent score |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Decoder | 116 | 102 | 102 | 1 | 1 | 12 | 1 | 0 | 102/103 (99.03%) |
| Reference | 171 | 161 | 160 | 5 | 5 | 5 | 0 | 1 | 160/161 (99.38%) |
| Shader parser, verification and lowering | 112 | 97 | 97 | 7 | 7 | 8 | 0 | 0 | 97/97 (100%) |
| **Total** | **399** | **360** | **359** | **13** | **13** | **25** | **1** | **1** | **359/361 (99.45%)** |

The 13 raw misses were all independently reviewed as equivalent. The
historical location table above applies except for the two shader branch
guards moved during the whole-stream verification fix.

- Shader `279:20`, entire guard replaced with false. `lift` is private and
  receives decoder output only. The decoder rejects negative displacements
  and targets outside instruction boundaries. The guard's original predicate
  is always false.
- Shader `280:21`, OR changed to AND. The same input contract makes both
  components false, so the combined predicate is still false.
- The 25 invalid entries are 22 missing `Default` implementations, one
  ambiguous trait call, and two unsupported OR operators in a let chain.
  The ledger maps every entry to the compiler log and mutation diff.
- Decoder `120:16`, `+=` changed to `*=`, built successfully and explicitly
  timed out after 300 seconds. A successfully decoded instruction leaves
  the initially zero byte offset at zero, so the loop repeats.
- Reference `235:16` applies the same change to the independent byte
  offset. The test process received SIGKILL after about 183 seconds, with
  no assertion failure. The mutant repeatedly appends the same instruction
  at offset zero. The signal's origin was not recorded. This entry remains
  in the denominator, separate from the explicit timeout.

The ignored `artifacts/mutation/source-aff46b0/` directory contains
`environment.log`, `exit-code`, `ledger.tsv`, `ledger-summary.txt`, and
the original `mutants.out/` reports with per-mutant logs and diffs.

| Artifact | SHA-256 |
| --- | --- |
| `mutants.out/mutants.json` | `1776e2a91e91f0e451ef6a24c483cda6251e2ae8561222bae665564d2edbe8a6` |
| `mutants.out/outcomes.json` | `ff258c717976d1f9cb74ce776413200418fff04ffe5f81fad30f8932a6335d24` |
| `ledger.tsv` | `fabd5488d0954f8778d118708f55e3818902d60788dcf4b877b108639513ae05` |

## Semantic and oracle campaigns at aff46b0

The [native semantic suite](../../tests/mutation/README.md) killed all 12
enumerated mutations with both Metal validation layers enabled on Apple M5.
Every unmodified GPU readback matched six hand-derived words and the CPU
reference. Each deliberately changed shader then ran successfully and
returned different words. R1 independently verified the scope, output and
30-second child process cutoff. Raw log
`artifacts/assurance-aff46b0/semantic.log` and exit 0.

The oracle-only campaign mutated all generated `vector_alu` and
`execute_lane` entries and ran the independent validation package alone.

```sh
CARGO_BUILD_JOBS=2 cargo mutants --file crates/ref/src/lib.rs \
  --re ' in (vector_alu|execute_lane)$|replace (vector_alu|execute_lane) ->' \
  --test-package xenora-semantic-review --jobs 2 --timeout 60 \
  --output artifacts/mutation/oracle-aff46b0
```

Of 44 mutations, 37 were caught and seven survived. The survivors were the
three changed operators in the FP32 operand guard at 287, the load or index
guard at 339, the output write check at 350, the resource SGPR test at 367,
and the outstanding transfer check at 392. They remain test gaps in the
independent oracle package. Source and native semantic evidence are
complete for the reviewed candidate. G7 remains open pending independent
oracle follow-up.


## Integrated source campaign at 4b9a4ce

R2 independently reviewed the full campaign at
`4b9a4cecf35c8d4d90d8ecdb9c1510e7bd260102`. Native ARM64 cargo-mutants
27.1.0 ran 399 mutations with two workers. The result matched the
historical per-module counts above: 359 behavioral kills out of 361
eligible non-equivalent mutants (99.45%), with 13 proven equivalents
and 25 invalid builds. The decoder timeout and the reference SIGKILL
remain in the denominator. No mutant was unexecuted.

The shell wrapper could not capture the raw child exit because it
assigned a read-only zsh variable. The completed console transcript
and full inventory and outcomes remain available. See
`artifacts/mutation/source-4b9a4ce/` for the raw campaign log, ledger,
exit capture note and independent G5 and G7 reports.

The implementation, tests, fixtures and scripts match `5c47900`, whose
separate integrated campaigns killed all 44 oracle-only and all 12
native semantic mutants. R2 also ran all 13 locked independent
validation tests successfully at `4b9a4ce`. G5 and G7 pass for
the bounded Phase 01 candidate.
