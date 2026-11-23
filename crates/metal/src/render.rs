// SPDX-License-Identifier: GPL-3.0-or-later

use std::ptr::NonNull;

use objc2::rc::autoreleasepool;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::sel;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLLoadAction,
    MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLShaderValidation, MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage, MTLViewport,
};

use super::{
    MetalError, NativeCompute, compile_options, complete_command, find_function, validate_source,
};

const MAX_PIXELS: usize = 4096;
const PIXEL_FORMAT: MTLPixelFormat = MTLPixelFormat::RGBA8Unorm;

impl NativeCompute {
    /// Draw one triangle with trusted MSL into a cleared RGBA8Unorm target.
    ///
    /// The entries are ordered vertex, then fragment. Vertex slot zero holds
    /// three clip-space float4 positions. Vertex slot one holds three float4
    /// RGBA colors. The stages must restrict accesses to these inputs and
    /// the single color output. Use only trusted MSL in a process with a wall
    /// deadline. The returned bytes are tightly packed in top-to-bottom rows.
    ///
    /// # Errors
    ///
    /// Returns an error for non-finite inputs, invalid source or extent, or
    /// Metal compilation, setup or completion failure.
    pub fn render_rgba8(
        &self,
        source: &str,
        entries: [&str; 2],
        vertices: &[[f32; 4]; 3],
        colors: &[[f32; 4]; 3],
        [width, height]: [usize; 2],
        clear: [u8; 4],
    ) -> Result<Vec<u8>, MetalError> {
        self.render_rgba8_sampled(
            source,
            entries,
            vertices,
            colors,
            [width, height],
            clear,
            None,
        )
    }

    /// Draw one triangle with trusted MSL and optional bound sampled texture into a cleared RGBA8Unorm target.
    ///
    /// When `sampled` is provided, the texture is bound to fragment texture slot 0 and
    /// the sampler to fragment sampler slot 0.
    ///
    /// # Errors
    ///
    /// Returns an error for non-finite inputs, invalid source or extent, or
    /// Metal compilation, setup or completion failure.
    #[allow(clippy::too_many_arguments)]
    pub fn render_rgba8_sampled(
        &self,
        source: &str,
        entries: [&str; 2],
        vertices: &[[f32; 4]; 3],
        colors: &[[f32; 4]; 3],
        [width, height]: [usize; 2],
        clear: [u8; 4],
        sampled: Option<(
            &crate::texture_pipeline::MetalRgba8Texture,
            &crate::sampler::MetalSampler,
        )>,
    ) -> Result<Vec<u8>, MetalError> {
        let pixels = width
            .checked_mul(height)
            .filter(|&count| (1..=MAX_PIXELS).contains(&count))
            .ok_or_else(|| MetalError::new("render bounds", "expected 1..=4096 pixels"))?;
        validate_source(source, &entries)?;
        if vertices
            .iter()
            .chain(colors)
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(MetalError::new(
                "render input",
                "expected finite vertex data",
            ));
        }

        autoreleasepool(|_| {
            let options = compile_options()?;
            let library = self.compile_library(source, &options)?;
            let vertex = find_function(&library, entries[0])?;
            let fragment = find_function(&library, entries[1])?;
            let descriptor = MTLRenderPipelineDescriptor::new();
            descriptor.setVertexFunction(Some(&vertex));
            descriptor.setFragmentFunction(Some(&fragment));
            // SAFETY: Slot zero is the only color attachment for this pipeline.
            unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) }
                .setPixelFormat(PIXEL_FORMAT);
            let pipeline = self
                .device
                .newRenderPipelineStateWithDescriptor_error(&descriptor)
                .map_err(|error| {
                    MetalError::new(
                        "newRenderPipelineStateWithDescriptor",
                        error.localizedDescription().to_string(),
                    )
                })?;
            verify_render_validation(&pipeline)?;

            // SAFETY: The nonzero dimensions are bounded by MAX_PIXELS, and
            // RGBA8Unorm is a renderable format on the selected Apple family.
            let texture_descriptor = unsafe {
                MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                    PIXEL_FORMAT,
                    width,
                    height,
                    false,
                )
            };
            texture_descriptor.setStorageMode(MTLStorageMode::Shared);
            texture_descriptor.setUsage(MTLTextureUsage::RenderTarget);
            let texture = self
                .device
                .newTextureWithDescriptor(&texture_descriptor)
                .ok_or_else(|| {
                    MetalError::new(
                        "newTextureWithDescriptor",
                        "render target allocation failed",
                    )
                })?;
            let pass = MTLRenderPassDescriptor::renderPassDescriptor();
            // SAFETY: Slot zero is the only color attachment for this pass.
            let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
            attachment.setTexture(Some(&texture));
            attachment.setLoadAction(MTLLoadAction::Clear);
            let [red, green, blue, alpha] = clear.map(|value| f64::from(value) / 255.0);
            attachment.setClearColor(MTLClearColor {
                red,
                green,
                blue,
                alpha,
            });
            attachment.setStoreAction(MTLStoreAction::Store);

            let positions = self.new_buffer(
                vertices
                    .map(|vertex| vertex.map(f32::to_bits))
                    .as_flattened(),
            )?;
            let colors =
                self.new_buffer(colors.map(|color| color.map(f32::to_bits)).as_flattened())?;
            let command = self.queue.commandBuffer().ok_or_else(|| {
                MetalError::new("commandBuffer", "render command buffer creation failed")
            })?;
            let encoder = command
                .renderCommandEncoderWithDescriptor(&pass)
                .ok_or_else(|| {
                    MetalError::new(
                        "renderCommandEncoderWithDescriptor",
                        "render encoder creation failed",
                    )
                })?;
            encoder.setRenderPipelineState(&pipeline);
            encoder.setViewport(MTLViewport {
                originX: 0.0,
                originY: 0.0,
                width: width as f64,
                height: height as f64,
                znear: 0.0,
                zfar: 1.0,
            });
            // SAFETY: Both buffers come from this device and each holds three
            // float4s. The trusted vertex entry may read only these records.
            // The render target format matches that of the pipeline.
            unsafe {
                encoder.setVertexBuffer_offset_atIndex(Some(&positions), 0, 0);
                encoder.setVertexBuffer_offset_atIndex(Some(&colors), 0, 1);
                if let Some((tex, samp)) = sampled {
                    encoder.setFragmentTexture_atIndex(Some(tex.raw()), 0);
                    encoder.setFragmentSamplerState_atIndex(Some(samp.raw()), 0);
                }
                encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3);
            }
            encoder.endEncoding();
            complete_command(&command)?;

            let region = MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width,
                    height,
                    depth: 1,
                },
            };
            let mut output = vec![0; pixels * 4];
            // SAFETY: GPU writes have completed, and the initialized output
            // allocation fits the entire shared texture with tightly packed rows.
            unsafe {
                texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                    NonNull::from(output.as_mut_slice()).cast(),
                    width * 4,
                    region,
                    0,
                );
            }
            Ok(output)
        })
    }
}

fn verify_render_validation(
    pipeline: &ProtocolObject<dyn MTLRenderPipelineState>,
) -> Result<(), MetalError> {
    if std::env::var("MTL_SHADER_VALIDATION").as_deref() != Ok("1") {
        return Ok(());
    }
    if !pipeline.respondsToSelector(sel!(shaderValidation)) {
        return Err(MetalError::new(
            "shaderValidation",
            "the runtime lacks the render pipeline state query",
        ));
    }
    // SAFETY: The selector exists and the newly created pipeline is private
    // to this thread. Its non-atomic property is read before encoding.
    if unsafe { pipeline.shaderValidation() } != MTLShaderValidation::Enabled {
        return Err(MetalError::new(
            "shaderValidation",
            "shader validation requested but inactive for the render pipeline",
        ));
    }
    Ok(())
}
