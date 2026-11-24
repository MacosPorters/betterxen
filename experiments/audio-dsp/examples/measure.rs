// SPDX-License-Identifier: GPL-3.0-or-later

use std::hint::black_box;
use std::time::Instant;
use xenora_audio_dsp::{Channels, Encoding, Format, Limits, Session};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => offline(),
        Some("--play") => native(),
        _ => Err("usage: measure [--play]".into()),
    }
}

fn offline() -> Result<(), Box<dyn std::error::Error>> {
    const BLOCK: usize = 256;
    const VOICES: usize = 8;
    const ITERATIONS: usize = 1500;
    let format = Format::new(48_000, Channels::Stereo)?;
    let session = Session::new(format, Limits::default())?;
    let inputs = (0..VOICES)
        .map(|_| session.open_stream(format))
        .collect::<Result<Vec<_>, _>>()?;
    let mut renderer = session.renderer()?;
    let pcm = [0x00, 0x04, 0x00, 0xfc].repeat(BLOCK);
    let mut output = [0.0; 2 * BLOCK];
    let mut times = Vec::with_capacity(ITERATIONS);
    let mut mix_times = Vec::with_capacity(ITERATIONS);
    let mut peak_bytes = 0;
    for _ in 0..100 {
        for &stream in &inputs {
            session.submit_pcm(stream, Encoding::S16Le, &pcm)?;
        }
        renderer.render(black_box(&mut output))?;
    }
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        for &stream in &inputs {
            session.submit_pcm(stream, Encoding::S16Le, &pcm)?;
        }
        let staged_at = Instant::now();
        renderer.render(black_box(&mut output))?;
        mix_times.push(staged_at.elapsed().as_nanos());
        times.push(start.elapsed().as_nanos());
    }
    for &stream in &inputs {
        session.submit_pcm(stream, Encoding::S16Le, &pcm)?;
    }
    peak_bytes = peak_bytes.max(session.metrics()?.reserved_sample_bytes);
    renderer.render(&mut output)?;
    let metrics = session.metrics()?;
    times.sort_unstable();
    mix_times.sort_unstable();
    assert_eq!((output[0], output[1]), (0.25, -0.25));
    assert_eq!(metrics.underrun_stream_frames, 0);
    println!("Offline: {ITERATIONS} × {BLOCK} frames, {VOICES} stereo streams, 48000 Hz");
    println!(
        "submit+mix p50={} ns p95={} ns; mix p50={} ns p95={} ns",
        times[ITERATIONS / 2],
        times[ITERATIONS * 95 / 100],
        mix_times[ITERATIONS / 2],
        mix_times[ITERATIONS * 95 / 100]
    );
    println!(
        "Reserved PCM payload at one fully staged block: {peak_bytes} bytes; staging high-water={} source frames",
        metrics.peak_staging_frames
    );
    Ok(())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;
    use xenora_audio_dsp::coreaudio::Playback;

    let format = Format::new(48_000, Channels::Stereo)?;
    let limits = Limits {
        frames_per_stream: 24_000,
        staging_frames: 24_000,
        ..Limits::default()
    };
    let session = Session::new(format, limits)?;
    let stream = session.open_stream(format)?;
    let pcm = (0..20_000)
        .flat_map(|frame| {
            let envelope = (frame as f32 / 500.0)
                .min(1.0)
                .min((19_999 - frame) as f32 / 500.0);
            let sample =
                (frame as f32 * 220.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.015 * envelope;
            [sample.to_le_bytes(); 2].into_iter().flatten()
        })
        .collect::<Vec<_>>();
    session.submit_pcm(stream, Encoding::F32Le, &pcm)?;
    let playback = Playback::start(&session, 256)?;
    std::thread::sleep(Duration::from_millis(260));
    let measured = playback.stop()?;
    let metrics = session.metrics()?;
    assert!(measured.buffer_callbacks > 0);
    assert!(measured.queue_clock_frames.is_some_and(|frames| frames > 0));
    assert_eq!(metrics.underrun_stream_frames, 0);
    assert_eq!(metrics.output_frames, measured.submitted_frames);
    println!("Native playback: {measured:?}");
    println!("Session: {metrics:?}");
    Ok(())
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn native() -> Result<(), Box<dyn std::error::Error>> {
    Err("native playback requires Apple Silicon macOS".into())
}
