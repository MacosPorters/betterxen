// SPDX-License-Identifier: GPL-3.0-or-later
// BetterXen addition to the vendored victory-shader crate (see ../../UPSTREAM.md): a public
// entry point for a raw compute program, with the same stages `translate` runs for a container.

use super::{HostShader, lower};
use crate::native::{HwStage, NativeProgram};
use crate::{ShaderError, io, resource, spill, structure};

/// Translates a compute program that is already a [`NativeProgram`] (machine code plus its
/// root layout) instead of one stored in a console shader container. Only the compute stage is
/// accepted: graphics stages need signatures that only a container carries.
pub fn translate_native(native: &NativeProgram) -> Result<HostShader, ShaderError> {
    if native.stage != HwStage::Cs {
        return Err(ShaderError::Unsupported {
            offset: 0,
            what: "translate_native takes compute programs only",
        });
    }
    // (what an earlier program's lowering on this thread left behind)
    lower::take_needs();
    let p = spill::build(native)?;
    let st = structure::structure(&p);
    let res = resource::track(&p, &native.layout, native.stage, st.as_ref().ok())?;
    let io = io::raise(&p, native)?;
    let st = st?;
    let lowered = lower::lower(&lower::Parts {
        native,
        p: &p,
        st: &st,
        io: &io,
        res: &res,
        inputs: Vec::new(),
        outputs: Vec::new(),
        prims: Vec::new(),
        mesh_stride: 0,
    })?;
    Ok(HostShader {
        stage: native.stage,
        dxil: lowered.dxil,
        user_data: lowered.user_data,
        static_samplers: super::static_samplers(&lowered.samplers, lowered.cmp_samplers),
        samplers: lowered.samplers,
        root_cbvs: lowered.root_cbvs,
        root_srvs: lowered.root_srvs,
        root_uavs: lowered.root_uavs,
        root_indexed_views: res.root_indexed_views,
    })
}
