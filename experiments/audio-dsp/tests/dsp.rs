// SPDX-License-Identifier: GPL-3.0-or-later

use std::thread;
use xenora_audio_dsp::{AudioError, Channels::*, Control, Encoding::*, Format, Limits, Session};

fn s16(samples: &[i16]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn fp(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn pcm_value(sample: i16) -> f32 {
    f32::from(sample) / 32768.0
}

fn verify_s16(actual: &[f32], expected: &[i16]) {
    assert_eq!(actual.len(), expected.len());
    for (&actual, &expected) in actual.iter().zip(expected) {
        assert!(
            (actual - pcm_value(expected)).abs() < 0.000_002,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn mixer_clips_after_accumulation_and_downmixes_stereo() {
    let out = Format::new(48_000, Mono).unwrap();
    let dsp = Session::new(out, Limits::default()).unwrap();
    let a = dsp.open_stream(out).unwrap();
    let b = dsp.open_stream(out).unwrap();
    let c = dsp
        .open_stream(Format::new(48_000, Stereo).unwrap())
        .unwrap();
    dsp.submit_pcm(a, S16Le, &s16(&[30000, -30000, 30000, 30000]))
        .unwrap();
    dsp.submit_pcm(b, S16Le, &s16(&[10000, -10000, 30000, 0]))
        .unwrap();
    dsp.submit_pcm(c, S16Le, &s16(&[0, 0, 0, 0, -30000, -30000, 32767, -32767]))
        .unwrap();
    let mut result = [0.0; 4];
    dsp.renderer().unwrap().render(&mut result).unwrap();
    assert_eq!(result[0], 1.0);
    verify_s16(&result[1..], &[-32768, 30000, 30000]);
}

#[test]
fn rational_resampler_continues_across_chunks_and_releases_the_last_frame() {
    let dsp = Session::new(Format::new(48_000, Stereo).unwrap(), Limits::default()).unwrap();
    let stream = dsp.open_stream(Format::new(32_000, Mono).unwrap()).unwrap();
    dsp.submit_pcm(stream, S16Le, &s16(&[0, 3000])).unwrap();
    let mut render = dsp.renderer().unwrap();
    let mut first = [0.0; 4];
    render.render(&mut first).unwrap();
    dsp.submit_pcm(stream, S16Le, &s16(&[6000, 9000, 12000]))
        .unwrap();
    let mut second = [0.0; 10];
    render.render(&mut second).unwrap();
    verify_s16(&first, &[0, 0, 2000, 2000]);
    verify_s16(
        &second,
        &[
            4000, 4000, 6000, 6000, 8000, 8000, 10000, 10000, 12000, 12000,
        ],
    );
    let mut silence = [1.0; 2];
    render.render(&mut silence).unwrap();
    assert_eq!(silence, [0.0; 2]);
    assert_eq!(dsp.metrics().unwrap().underrun_stream_frames, 1);
}

#[test]
fn stereo_downsampling_keeps_channels_independent() {
    let dsp = Session::new(Format::new(32_000, Stereo).unwrap(), Limits::default()).unwrap();
    let stream = dsp
        .open_stream(Format::new(48_000, Stereo).unwrap())
        .unwrap();
    dsp.submit_pcm(
        stream,
        S16Le,
        &s16(&[
            0, 12000, 2000, 8000, 4000, 4000, 6000, 0, 8000, -4000, 10000, -8000, 12000, -12000,
        ]),
    )
    .unwrap();
    let mut result = [0.0; 10];
    dsp.renderer().unwrap().render(&mut result).unwrap();
    verify_s16(
        &result,
        &[0, 12000, 3000, 6000, 6000, 0, 9000, -6000, 12000, -12000],
    );
}

#[test]
fn block_partition_does_not_change_the_offline_output() {
    let input = s16(&(0..1024).collect::<Vec<_>>());
    let capture = |blocks: &[usize]| {
        let dsp = Session::new(Format::new(48_000, Mono).unwrap(), Limits::default()).unwrap();
        let stream = dsp.open_stream(Format::new(44_100, Mono).unwrap()).unwrap();
        dsp.submit_pcm(stream, S16Le, &input).unwrap();
        let mut render = dsp.renderer().unwrap();
        let mut output = vec![0.0; blocks.iter().sum()];
        let mut at = 0;
        for &size in blocks {
            render.render(&mut output[at..at + size]).unwrap();
            at += size;
        }
        output
    };
    let output = capture(&[1000]);
    assert_eq!(output, capture(&[37, 128, 3, 789, 43]));
    for k in 0..=6 {
        assert_eq!(output[k * 160], pcm_value((k * 147) as i16));
    }
}

#[test]
fn simultaneous_streams_start_on_the_same_clock() {
    let format = Format::new(48_000, Mono).unwrap();
    let dsp = Session::new(format, Limits::default()).unwrap();
    for (start, samples) in [(1, &[1000, 2000][..]), (2, &[4000, 8000]), (6, &[16000])] {
        let stream = dsp.open_stream(format).unwrap();
        dsp.control(stream, Control::StartAt(start)).unwrap();
        dsp.submit_pcm(stream, S16Le, &s16(samples)).unwrap();
    }
    let mut output = [0.0; 8];
    dsp.renderer().unwrap().render(&mut output).unwrap();
    verify_s16(&output, &[0, 1000, 6000, 8000, 0, 0, 16000, 0]);
}

#[test]
fn ordered_controls_apply_before_the_block_and_pause_holds_samples() {
    let format = Format::new(48_000, Stereo).unwrap();
    let dsp = Session::new(format, Limits::default()).unwrap();
    let stream = dsp.open_stream(format).unwrap();
    assert_eq!(dsp.control(stream, Control::Mute(true)), Ok(0));
    assert_eq!(dsp.control(stream, Control::Pause), Ok(1));
    assert_eq!(
        dsp.submit_pcm(stream, F32Le, &fp(&[0.75, -0.5, 0.5, 0.25])),
        Ok(2)
    );
    let mut render = dsp.renderer().unwrap();
    let mut out = [90.0; 2];
    render.render(&mut out).unwrap();
    assert_eq!(out, [0.0; 2]);
    assert_eq!(dsp.metrics().unwrap().buffered_source_frames, 2);
    dsp.control(stream, Control::Mute(false)).unwrap();
    dsp.control(stream, Control::Gain([0.5, 0.25])).unwrap();
    dsp.control(stream, Control::StartAt(1)).unwrap();
    render.render(&mut out).unwrap();
    assert_eq!(out, [0.375, -0.125]);
    assert_eq!(dsp.metrics().unwrap().last_applied, Some(5));
}

#[test]
fn stale_foreign_and_wrong_stream_handles_are_rejected() {
    let format = Format::new(48_000, Mono).unwrap();
    let dsp = Session::new(format, Limits::default()).unwrap();
    let other = Session::new(format, Limits::default()).unwrap();
    let stream = dsp.open_stream(format).unwrap();
    let peer = dsp.open_stream(format).unwrap();
    let outside = other.open_stream(format).unwrap();
    let buffer = dsp.allocate_pcm(stream, S16Le, &s16(&[1])).unwrap();
    assert_eq!(
        dsp.submit_buffer(peer, buffer),
        Err(AudioError::WrongStream)
    );
    assert_eq!(
        dsp.control(outside, Control::Pause),
        Err(AudioError::InvalidHandle)
    );
    dsp.submit_buffer(stream, buffer).unwrap();
    assert_eq!(dsp.retire_buffer(buffer), Err(AudioError::BufferInFlight));
    dsp.renderer().unwrap().render(&mut [0.0]).unwrap();
    assert_eq!(dsp.retire_buffer(buffer), Err(AudioError::InvalidHandle));
    dsp.close_stream(stream).unwrap();
    let replacement = dsp.open_stream(format).unwrap();
    assert_ne!(stream, replacement);
    assert_eq!(dsp.stream_format(stream), Err(AudioError::InvalidHandle));
}

#[test]
fn invalid_frames_and_overruns_leave_existing_audio_intact() {
    let format = Format::new(48_000, Mono).unwrap();
    let limits = Limits {
        streams: 1,
        frames_per_stream: 2,
        commands: 1,
        ..Limits::default()
    };
    let dsp = Session::new(format, limits).unwrap();
    let stream = dsp.open_stream(format).unwrap();
    assert_eq!(dsp.open_stream(format), Err(AudioError::StreamLimit));
    assert_eq!(
        dsp.submit_pcm(stream, S16Le, &[1]),
        Err(AudioError::InvalidLength)
    );
    assert_eq!(
        dsp.submit_pcm(stream, F32Le, &fp(&[f32::NAN])),
        Err(AudioError::InvalidSample)
    );
    dsp.submit_pcm(stream, S16Le, &s16(&[1000])).unwrap();
    assert_eq!(
        dsp.submit_pcm(stream, S16Le, &s16(&[2000])),
        Err(AudioError::QueueFull)
    );
    assert_eq!(
        dsp.submit_pcm(stream, S16Le, &s16(&[3000, 4000])),
        Err(AudioError::BufferBudget)
    );
    let mut out = [0.0; 2];
    dsp.renderer().unwrap().render(&mut out).unwrap();
    verify_s16(&out, &[1000, 0]);
    let metrics = dsp.metrics().unwrap();
    assert_eq!(metrics.command_overruns, 1);
    assert_eq!(metrics.buffer_overruns, 1);
    assert_eq!(metrics.underrun_stream_frames, 1);
    assert_eq!(metrics.live_buffers, 0);
}

#[test]
fn concurrent_producers_render_in_ticket_order() {
    let format = Format::new(48_000, Mono).unwrap();
    let dsp = Session::new(format, Limits::default()).unwrap();
    let stream = dsp.open_stream(format).unwrap();
    let mut receipts = thread::scope(|scope| {
        (1..=16)
            .map(|sample| {
                let dsp = &dsp;
                scope.spawn(move || {
                    (
                        dsp.submit_pcm(stream, S16Le, &s16(&[sample])).unwrap(),
                        sample,
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    receipts.sort_unstable();
    let expected = receipts
        .into_iter()
        .map(|(_, sample)| sample)
        .collect::<Vec<_>>();
    let mut output = [0.0; 16];
    dsp.renderer().unwrap().render(&mut output).unwrap();
    verify_s16(&output, &expected);
}

#[test]
fn lifecycle_releases_reservations_in_one_hundred_cycles() {
    let format = Format::new(48_000, Mono).unwrap();
    for _ in 0..100 {
        let dsp = Session::new(format, Limits::default()).unwrap();
        let stream = dsp.open_stream(format).unwrap();
        let staged = dsp.allocate_pcm(stream, S16Le, &s16(&[1, 2])).unwrap();
        dsp.submit_pcm(stream, S16Le, &s16(&[3])).unwrap();
        let mut render = dsp.renderer().unwrap();
        assert!(matches!(dsp.renderer(), Err(AudioError::RendererBusy)));
        dsp.close_stream(stream).unwrap();
        assert_eq!(dsp.retire_buffer(staged), Err(AudioError::InvalidHandle));
        assert_eq!(dsp.metrics().unwrap().commands_cancelled, 1);
        dsp.close().unwrap();
        dsp.close().unwrap();
        assert_eq!(render.render(&mut [0.0]), Err(AudioError::Closed));
        let metrics = dsp.metrics().unwrap();
        assert_eq!(
            (
                metrics.live_streams,
                metrics.live_buffers,
                metrics.pending_commands,
                metrics.reserved_sample_bytes,
                metrics.buffered_source_frames
            ),
            (0, 0, 0, 0, 0)
        );
    }
}

#[test]
fn fractional_resampling_refuses_a_one_frame_capacity() {
    let output = Format::new(48_000, Mono).unwrap();
    let limits = Limits {
        frames_per_stream: 1,
        ..Limits::default()
    };
    let dsp = Session::new(output, limits).unwrap();
    assert_eq!(
        dsp.open_stream(Format::new(32_000, Mono).unwrap()),
        Err(AudioError::InvalidLimits)
    );
    assert_eq!(
        dsp.open_stream(Format::new(72_000, Mono).unwrap()),
        Err(AudioError::InvalidLimits)
    );
    let matching = dsp.open_stream(output).unwrap();
    dsp.submit_pcm(matching, S16Le, &s16(&[1000])).unwrap();
    let mut result = [0.0];
    dsp.renderer().unwrap().render(&mut result).unwrap();
    verify_s16(&result, &[1000]);
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn coreaudio_rejects_a_short_block_before_touching_the_dsp() {
    use xenora_audio_dsp::coreaudio::{Playback, PlaybackError};
    let output = Format::new(192_000, Stereo).unwrap();
    let dsp = Session::new(output, Limits::default()).unwrap();
    let stream = dsp.open_stream(output).unwrap();
    dsp.submit_pcm(stream, S16Le, &s16(&[1000, 2000])).unwrap();
    assert!(matches!(
        Playback::start(&dsp, 64),
        Err(PlaybackError::InvalidBlockSize)
    ));
    assert_eq!(dsp.metrics().unwrap().output_frames, 0);
    let mut samples = [0.0; 2];
    dsp.renderer().unwrap().render(&mut samples).unwrap();
    verify_s16(&samples, &[1000, 2000]);
}
