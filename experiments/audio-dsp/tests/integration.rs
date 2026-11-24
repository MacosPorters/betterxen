// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "xenora")]

use xenora_audio_dsp::{Channels, Format, Limits, Session, xenora::*};
use xenora_ffi::{AudioSubsystem, XENORA_AUDIO_FORMAT_PCM_S16LE};
use xenora_loader::cpu::{CpuState, ExitReason, Register};
use xenora_loader::imports::{ImportRegistry, Win64Args};
use xenora_loader::memory::{GuestMemory, MemoryPermissions};

#[test]
fn guest_registry_checks_ranges_handles_and_frame_atomicity() {
    let format = Format::new(48_000, Channels::Stereo).unwrap();
    let dsp = Session::new(format, Limits::default()).unwrap();
    let stream = dsp.open_stream(format).unwrap();
    let mut registry = ImportRegistry::new();
    register_fixture_services(&mut registry, &dsp).unwrap();
    assert!(register_fixture_services(&mut registry, &dsp).is_err());
    let mut memory = GuestMemory::new();
    memory
        .map_region_with_data(
            0x1000,
            4096,
            MemoryPermissions::READ_WRITE,
            &[0, 0x80, 0xff, 0x7f],
        )
        .unwrap();
    let mut cpu = CpuState::default();
    cpu.set(Register::Rcx, stream.raw());
    cpu.set(Register::Rdx, 0x1000);
    cpu.set(Register::R8, 3);
    cpu.set(Register::R9, 1);
    let submit = registry.binding(MODULE, SUBMIT_PCM).unwrap();
    assert_eq!(
        (submit.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory),
        Ok(INVALID_DATA)
    );
    cpu.set(Register::R8, 4);
    cpu.set(Register::Rdx, 0x1ffe);
    assert!(matches!(
        (submit.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory),
        Err(ExitReason::MemoryFault(_))
    ));
    assert_eq!(dsp.metrics().unwrap().pending_commands, 0);
    cpu.set(Register::Rdx, 0x1000);
    assert_eq!(
        (submit.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory),
        Ok(OK)
    );
    let mut pcm = [0.0; 2];
    dsp.renderer().unwrap().render(&mut pcm).unwrap();
    assert_eq!(pcm, [-1.0, 32767.0 / 32768.0]);
    dsp.close_stream(stream).unwrap();
    assert_eq!(
        (submit.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory),
        Ok(INVALID_HANDLE)
    );
}

#[test]
fn gate6_transfer_reuses_processed_pcm_and_preserves_failed_submission() {
    let format = Format::new(48_000, Channels::Mono).unwrap();
    let dsp = Session::new(
        format,
        Limits {
            commands: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    let stream = dsp.open_stream(format).unwrap();
    let mut gate = AudioSubsystem {
        channels: 1,
        volume: 0.5,
        format: XENORA_AUDIO_FORMAT_PCM_S16LE,
        ..AudioSubsystem::default()
    };
    gate.submit(&[0, 0x40, 0, 0xc0]);
    assert_eq!(transfer_gate6(&mut gate, &dsp, stream, 1), Ok(1));
    assert!(transfer_gate6(&mut gate, &dsp, stream, 1).is_err());
    assert_eq!(gate.queue.len(), 2);
    let mut render = dsp.renderer().unwrap();
    let mut output = [0.0];
    render.render(&mut output).unwrap();
    assert_eq!(output, [0.25]);
    assert_eq!(transfer_gate6(&mut gate, &dsp, stream, 1), Ok(1));
    render.render(&mut output).unwrap();
    assert_eq!(output, [-0.25]);
    assert!(gate.queue.is_empty());
}

#[test]
fn gate6_transfer_makes_progress_with_a_held_interpolation_frame() {
    let output = Format::new(48_000, Channels::Mono).unwrap();
    let limits = Limits {
        frames_per_stream: 2,
        ..Limits::default()
    };
    let dsp = Session::new(output, limits).unwrap();
    let stream = dsp
        .open_stream(Format::new(32_000, Channels::Mono).unwrap())
        .unwrap();
    let mut gate = AudioSubsystem {
        channels: 1,
        sample_rate: 32_000,
        ..AudioSubsystem::default()
    };
    gate.submit(&[0, 0, 0, 0x30, 0, 0x60]);
    assert_eq!(transfer_gate6(&mut gate, &dsp, stream, 1), Ok(1));
    let mut render = dsp.renderer().unwrap();
    let mut output = [1.0];
    render.render(&mut output).unwrap();
    assert_eq!(output, [0.0]);
    assert_eq!(transfer_gate6(&mut gate, &dsp, stream, 2), Ok(1));
    render.render(&mut output).unwrap();
    assert_eq!(output, [0.25]);
}
