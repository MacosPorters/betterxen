// SPDX-License-Identifier: GPL-3.0-or-later

//! Adapters for explicitly authored Xenora workloads and the Gate 6 PCM queue.
//! Retail guest imports remain outside this module.

use crate::{AudioError, Channels, Control, Encoding, Format, Session, StreamHandle};
use xenora_ffi::{AudioSubsystem, XENORA_AUDIO_FORMAT_PCM_F32LE, XENORA_AUDIO_FORMAT_PCM_S16LE};
use xenora_loader::cpu::ExitReason;
use xenora_loader::import_table::ImportName;
use xenora_loader::imports::{ImportRegistry, RegistryError, ServiceStatus};
use xenora_loader::runtime::Call;

pub const MODULE: &str = "xenora_audio_fixture.dll";
pub const SUBMIT_PCM: &str = "SubmitPcm";
pub const SCHEDULE_START: &str = "ScheduleStart";
pub const OK: u64 = 0;
pub const INVALID_HANDLE: u64 = 1;
pub const INVALID_DATA: u64 = 2;
pub const RESOURCE_EXHAUSTED: u64 = 3;
pub const SESSION_ERROR: u64 = 4;

/// Transfer whole Gate 6 frames using its existing PCM storage and format.
pub fn transfer_gate6(
    audio: &mut AudioSubsystem,
    session: &Session,
    stream: StreamHandle,
    frame_limit: usize,
) -> crate::Result<usize> {
    let channels = match audio.channels {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        _ => return Err(AudioError::InvalidFormat),
    };
    let format = Format::new(audio.sample_rate, channels)?;
    if format != session.stream_format(stream)? {
        return Err(AudioError::InvalidFormat);
    }
    let encoding = match audio.format {
        XENORA_AUDIO_FORMAT_PCM_S16LE => Encoding::S16Le,
        XENORA_AUDIO_FORMAT_PCM_F32LE => Encoding::F32Le,
        _ => return Err(AudioError::InvalidFormat),
    };
    let stride = encoding.frame_bytes(channels);
    if !audio.queue.len().is_multiple_of(stride) {
        return Err(AudioError::InvalidLength);
    }
    let count = frame_limit
        .min(audio.queue.len() / stride)
        .min(session.available_pcm_frames(stream)?);
    if count != 0 {
        let bytes = count * stride;
        session.submit_pcm(stream, encoding, &audio.queue.make_contiguous()[..bytes])?;
        audio.queue.drain(..bytes);
    }
    Ok(count)
}

/// Register the documented Xenora synthetic ABI for a single session.
/// Existing handlers are never replaced.
pub fn register_fixture_services(
    registry: &mut ImportRegistry,
    session: &Session,
) -> Result<(), RegistryError> {
    for symbol in [SUBMIT_PCM, SCHEDULE_START] {
        if registry.has_binding(MODULE, symbol) {
            return Err(RegistryError::Duplicate {
                module: MODULE.into(),
                symbol: symbol.into(),
            });
        }
    }

    let output = session.clone();
    registry.try_register(
        MODULE,
        ImportName::Name(SUBMIT_PCM.into()),
        ServiceStatus::Implemented,
        Box::new(move |args, cpu, memory| {
            let call = Call::new(args, cpu, memory);
            let stream = StreamHandle::from_raw(call.u64(0)?);
            let address = call.u64(1)?;
            let length = call.u64(2)? as usize;
            let encoding = match call.u32(3)? {
                1 => Encoding::S16Le,
                2 => Encoding::F32Le,
                _ => return Ok(INVALID_DATA),
            };
            let limit = match output.max_pcm_bytes(stream, encoding) {
                Ok(limit) => limit,
                Err(err) => return Ok(error_status(err)),
            };
            let format = match output.stream_format(stream) {
                Ok(format) => format,
                Err(err) => return Ok(error_status(err)),
            };
            let alignment = match encoding {
                Encoding::S16Le => 2,
                Encoding::F32Le => 4,
            };
            if address == 0
                || !address.is_multiple_of(alignment)
                || length == 0
                || !length.is_multiple_of(encoding.frame_bytes(format.channels()))
                || length > limit
            {
                return Ok(INVALID_DATA);
            }
            let mut bytes = vec![0; length];
            memory
                .read_bytes(address, &mut bytes)
                .map_err(ExitReason::MemoryFault)?;
            Ok(output
                .submit_pcm(stream, encoding, &bytes)
                .map_or_else(error_status, |_| OK))
        }),
    )?;

    let output = session.clone();
    registry.try_register(
        MODULE,
        ImportName::Name(SCHEDULE_START.into()),
        ServiceStatus::Implemented,
        Box::new(move |args, cpu, memory| {
            let call = Call::new(args, cpu, memory);
            let stream = StreamHandle::from_raw(call.u64(0)?);
            let frame = call.u64(1)?;
            Ok(output
                .control(stream, Control::StartAt(frame))
                .map_or_else(error_status, |_| OK))
        }),
    )
}

fn error_status(error: AudioError) -> u64 {
    match error {
        AudioError::InvalidHandle | AudioError::WrongStream => INVALID_HANDLE,
        AudioError::InvalidFormat
        | AudioError::InvalidLength
        | AudioError::InvalidSample
        | AudioError::InvalidGain
        | AudioError::InvalidLimits => INVALID_DATA,
        AudioError::QueueFull | AudioError::BufferBudget | AudioError::StreamLimit => {
            RESOURCE_EXHAUSTED
        }
        _ => SESSION_ERROR,
    }
}
