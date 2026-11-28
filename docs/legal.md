# Clean-room scope

BetterXen does not circumvent console security, DRM, or activation; does not load encrypted or protected content; does not use leaked or stolen material; and does not redistribute proprietary binaries or SDKs. It is a research project built from public documentation, synthetic workloads and, under the conditions in [AGENTS.md](../AGENTS.md) §6, observed interface contracts from target builds.

## User-provided game files

Users obtain and supply their own files. BetterXen evaluates file structure,
supported features, compatibility and safe execution. Game import does
not check ownership, request receipts or account verification, investigate
acquisition, or block a technically supported file solely because its
origin is unknown. BetterXen does not distribute or host commercial game
files or provide acquisition services.

## Project contributions and testing

Observed interface contracts may be implemented for interoperability. Each PR
must label observed contracts and include a synthetic conformance test for
each. Do not copy proprietary header, SDK, or binary text. The accountable
maintainer records the acquisition and license basis for any target build in a
maintainer-only record. Contributors sign no attestations.

Other project contributions and project-run tests must use original or
authorized materials with compatible licenses and traceable provenance. The
application import path has no provenance check.

This is not legal advice. Contributors should know the laws that apply to them,
including anti-circumvention provisions such as DMCA section 1201 where
applicable.
