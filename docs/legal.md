# Legal Scope: Authorization, Confidentiality & Redistribution

This document is authoritative for what material the project may use, under whose permission, and what may be released. Operational rules for contributors and agents are in [AGENTS.md](../AGENTS.md) §4–6; dependency licensing is in [licenses.md](licenses.md).

BetterXen does not circumvent console security, DRM, or activation; does not load encrypted or protected content; does not use leaked or stolen material; and does not redistribute proprietary binaries or SDKs. It is built from public documentation, publicly licensed software, independently authored synthetic workloads and, only within an authorization record, restricted material.

## User-provided game files

Users obtain and supply their own files. BetterXen evaluates file structure,
supported features, compatibility and safe execution. Game import does
not check ownership, request receipts or account verification, investigate
acquisition, or block a technically supported file solely because its
origin is unknown. BetterXen does not distribute or host commercial game
files or provide acquisition services.

This describes how the application treats its users' files. It gives the
project, its contributors or its agents no permission to use any material in
development or testing.

## Authorized and unauthorized material

**Restricted material** is any target build or title content (however obtained), and any SDK, tool, documentation or data that is not public or is available only under an agreement. A publicly downloadable vendor package used within its published license terms is not restricted material; the per-file and per-component license rules apply to it.

Restricted material is **authorized** for an activity only when all of these hold:

- the people and systems involved have lawful access to it;
- a written grant from the rights holder, or from someone with authority to grant it for them, or license terms that expressly allow it, cover that activity, and applicable law permits it;
- it is not leaked, stolen or distributed without authorization, and neither are the tools used on it;
- the activity needs no decryption and no circumvention of security, DRM, licensing or activation controls.

Otherwise it is **unauthorized**. Non-public material is not unauthorized merely because it is non-public. Equally, none of the following is a grant: employment or affiliation, possession or purchase of a copy, a maintainer's approval, a contributor's assurance, or the fact that a technical result would be useful. No record can authorize leaked or stolen material or circumvention.

A grant covers only what its grantor controls. Where another party's terms also bind the material (for example a platform SDK agreement covering code or interfaces inside a title), the record states whether they are covered. If that is not shown, treat the permission as unknown.

## Authorization records

One record is reused for all work inside its scope, instead of repeated per-contributor attestations. Contributors sign no attestations; the DCO sign-off on commits still applies.

### Roles

- **Grantor:** the rights holder, or a person with authority to grant permission for them.
- **Custodian:** an accountable maintainer who keeps the record and answers what it covers. The custodian records a grant; they cannot create one.
- **Checker:** a second maintainer who confirms that the cited instrument supports each permission marked granted. A custodian's own check is never enough.

### Storage

Records, agreements and the material itself are kept in access-restricted storage chosen by the maintainers, **outside this repository and its GitHub issues, pull requests and discussions**. The only public trace is an opaque record ID (such as `BX-AUTH-0001`, carrying no title, company or build information).

This repository does not contain, and this document does not assert the existence of, any authorization record.

### Contents

| Field | Records |
| --- | --- |
| Record ID | Opaque public identifier |
| Material | What is covered, precisely enough to exclude everything else |
| Grantor and authority | Who granted, and why they may |
| Instrument | Reference to the agreement, license clause or written permission, and the acquisition basis |
| Third-party terms | Other terms binding the material and whether they are covered |
| People and environments | Who may act; which machines, tools and services |
| Custodian and checker | With the date of each confirmation |
| Dates | Effective, review or expiry, revocation |

Each permission is recorded separately as **granted** (with conditions), **not granted** or **unknown**. Only `granted` permits anything; no permission implies another.

| Permission | Covers |
| --- | --- |
| Access | Obtaining and holding the material |
| Inspection and analysis | Static inspection, call-site analysis, runtime diagnostics |
| Compatibility testing | Running the material under BetterXen |
| External AI processing | Sending named kinds of data to named services, with their retention and training terms |
| Publication of implementations | Releasing independently authored compatibility code and tests publicly under `GPL-3.0-or-later` |
| Sharing derived findings | Releasing contracts, measurements or diagnostics, and to which audience |

A record is **active** once the checker has confirmed it. Past its review date, or once revoked or disputed, it permits nothing until confirmed again.

### Using a record

- A pull request that relies on restricted material cites the record ID and nothing else from the record.
- Before merge, the custodian or checker confirms on the PR that the record is active and covers that PR's activities, including publication of what it contains. This confirmation is not a code review and does not replace [AGENTS.md](../AGENTS.md) §7.
- Work inside an active record's scope needs no further approval or attestation.
- If a permission is missing or unknown, only the work that depends on it stops: no further observation, no new contracts derived from the material, and no merge. Public, synthetic and unrelated engineering continues, including conflict fixes and defect corrections that need no restricted input.
- Never fill a gap by assumption, and never describe a pending record as obtained.

## External AI services

Restricted material and data derived from it (binaries, disassembly, memory dumps, traces, assets, confidential diagnostics or documentation) must not be sent to an external AI service unless the record grants external AI processing for that service and data. This includes coding agents that read local files. Without that grant, use approved local tooling or sanitized synthetic reproductions that reveal nothing the record does not allow to be shared.

## Publication, confidentiality and redistribution

- Permission to inspect or test is not permission to publish. This repository is public: pushing a branch, opening a PR or posting a comment is publication.
- Publish only independently written code and tests, and findings the record allows. Never publish proprietary code, header, SDK or binary text, assets, binaries, build identifiers, records or confidential agreements.
- Honor confidentiality obligations even where they are stricter than this document.
- If restricted content appears where it should not, tell a maintainer privately ([SECURITY.md](../SECURITY.md)). Do not quote it.

## Contributor exposure and separation

Access to confidential material under an authorization does **not** by itself exclude anyone from contributing. Such a contributor contributes only what their obligations and the record permit, and does not carry information from one authorization into work it does not cover.

Anyone who has accessed leaked or unauthorized Xbox material must not contribute. Do not post details of that material anywhere in the project.

## Project contributions and testing

Observed interface contracts may be implemented for interoperability. Each PR
must label observed contracts and include a synthetic conformance test for
each. Do not copy proprietary header, SDK, or binary text. The rules and
evidence labels are in [AGENTS.md](../AGENTS.md) §6.

All project contributions and project-run tests must use original or
authorized materials with compatible licenses and traceable provenance. The
application import path has no provenance check.

## Superseded decisions and earlier evidence

Earlier documents are left as written. These points take effect when this document is merged:

- **#483 deferral of Xbox `IDStorage*X`:** the requirement to wait for a public license-compatible source is replaced by: a public license-compatible source, **or** an observed contract under an active record that grants analysis and publication. The proprietary header `dstorage_xs.h` is still never copied. This authorizes no particular implementation.
- **Evidence recorded earlier from restricted material** (for example the target-workload shader scan in [the roadmap](roadmap.md) and [ADR 0008](adr/0008-m2-shader-format-gate.md)): the text stays as history. It does not show that a record exists. Do not extend that work until a custodian confirms an active record covers it.
- **"Project-authorized target build"** in earlier rules and PR discussions now means a build with an active record. A maintainer's earlier statement that a build qualified is not a record.

## Not legal advice

This is not legal advice. Contributors should know the laws that apply to them,
including anti-circumvention provisions such as DMCA section 1201 where
applicable. Whether a statutory interoperability exception applies is a
question for counsel, not for a contributor or an agent.
