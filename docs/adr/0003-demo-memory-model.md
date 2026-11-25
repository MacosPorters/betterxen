# 0003. Use disjoint lane-indexed buffers for the first fixture

Status: **Retained for the paused native implementation.** [ADR 0005](0005-crossover-hybrid-direction.md) supersedes native-first project direction. This contract still governs its existing synthetic fixtures and dependency boundaries; it does not define CrossOver integration or authorize new native work.

For the synthetic compute fixture, the resource beginning at SGPR 0 denotes the read-only input words and the one beginning at SGPR 4 denotes the output words. VGPR 0 starts at the lane index and must remain unchanged when used as a resource index. Each allocation has the same word count, and each dispatched invocation accesses only its own lane's word. The fixture ABI provides no raw resource descriptor bits or guest virtual addresses.

The compiler must reject descriptor forms, offsets, index changes and control flow outside this contract. Both engines must honor the load dependency and completion order. The host reads output after command completion. Supporting shared writable buffers will require a new ordering and aliasing contract.

AMD RDNA 2 specifies implicit outstanding memory waits at program termination (class B, CLAIM-RDNA2-TRANSFER-WAITS). This fixture policy requires explicit corresponding wait instructions before termination to keep the bounded execution model auditable. See the [RDNA 2 source notes](../research/rdna2.md).
