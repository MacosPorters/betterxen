//! Bounded execution of trusted Metal compute kernels and render stages.

#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
compile_error!("Xenora's Metal backend requires ARM64 macOS");

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
mod compute;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub mod sampler;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub mod texture_pipeline;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub use compute::{
    BlendDraw, BlendFactor, BlendOperation, BlendPass, BlendState, ColorWriteMask, DepthCompare,
    DepthDraw, DepthFill, DepthOutput, DepthPass, DepthState, GuestBuffer, GuestBufferView,
    IndexBuffer, IndexedDraw, IndexedPass, MetalError, NativeCompute, RunProfile, Submission,
    SubmissionGate,
};

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub use sampler::MetalSampler;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub use texture_pipeline::{
    BindingSlot, BoundSampledTexture, MetalRgba8Texture, Rgba8MipLevel,
    Rgba8TextureDesc as MetalRgba8TextureDesc,
};
