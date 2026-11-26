# Concurrent visibility: exploratory only

The proposed CPU-after-submit/GPU-spin and GPU-store/CPU-spin experiment intentionally omits the synchronization used by ordinary D3D12 applications. It is excluded from this protocol's executables and acceptance gates. No result from it is claimed here.

Microsoft's [Map contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-map) requires the CPU to finish writes before GPU execution in the persistent-map usage model and cautions against assuming CPU/GPU atomic guarantees. A CPU store fence, volatile load or a shader barrier alone does not create an API-guaranteed interprocessor rendezvous. [ALLOW_SIMULTANEOUS_ACCESS](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_resource_flags) is not permission for conflicting concurrent CPU/GPU writes to the same location.

A positive result would not prove a mid-flight observation: the GPU may start after the CPU store, commands may be delayed or serialized, and an implementation may synchronize/copy at a different point. A negative result would not prove shadow copying: CPU/GPU cache behavior, compiler hoisting, shader scheduling and unsupported atomic/visibility assumptions can all prevent progress. Neither outcome establishes physical page identity.

Any future exploratory implementation must be separate, explicitly opted into, bounded in shader iterations and process duration, and must preserve its exact compiled shader, scheduling evidence and outcomes without counting them as conformance passes/failures. A bounded spin may still monopolize a device or encounter watchdog limits; do not make an unbounded poll part of the shared suite. Establish actual GPU start/progress independently before interpreting apparent mid-flight behavior, and keep any conclusion conditional on that evidence.

The standard test instead completes CPU stores before dispatch, waits on a queue fence after the shader, and reads the existing mapped output pointer. This tests application-usable synchronized coherence without claiming to identify its physical implementation.
