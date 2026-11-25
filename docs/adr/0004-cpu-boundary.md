# 0004. Keep CPU recompilation outside the graphics dependency graph

Status: **Retained for the paused native implementation.** [ADR 0005](0005-crossover-hybrid-direction.md) supersedes native-first project direction. This contract still governs its existing synthetic fixtures and dependency boundaries; it does not define CrossOver integration or authorize new native work.

Phase 01 accepts synthetic shader code and explicit buffer bindings. A later guest CPU runtime will submit graphics commands using opaque resource handles, addresses, lengths and access modes. The graphics side will resolve addresses through checked mappings, retain resources through command completion and return fence completions or interrupt events via the command interface. CPU code may consume that interface. Graphics crates may depend on shared boundary types and graphics crates only, with no dependency on guest CPU or host x86-specific code.

Keep guest address arithmetic separate from host virtual addresses. Validate ranges before constructing a Metal view and retain the backing allocation for all queued users. Define aliasing and synchronization before supporting shared writable mappings. The first fixture uses independent input and output buffers and has no guest virtual address space.

See [CPU research](../research/cpu-boundary.md) for the host and guest evidence and [the interface contract](../interfaces.md) for the current fixture.
