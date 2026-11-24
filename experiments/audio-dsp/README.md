# Native audio DSP experiment

This Rust library mixes bounded PCM streams to deterministic offline output or the
default CoreAudio playback queue on Apple Silicon. The optional `xenora` feature
exposes the mixer through two original synthetic runtime services and can ingest
PCM from the existing Gate 6 queue.

## Build and test

Run from this directory with Rust 1.100 or later on Apple Silicon macOS.

```sh
cargo test --locked --target aarch64-apple-darwin --all-features
cargo clippy --locked --target aarch64-apple-darwin --all-features --all-targets -- -D warnings
cargo fmt --check
cargo run --locked --release --target aarch64-apple-darwin --example measure
```

The offline benchmark submits and mixes eight stereo PCM streams. Pass
`-- --play` to the same example for a brief, quiet tone on the default CoreAudio
output, with queue timing and underrun counters. Normal tests stay silent.

## Offline rendering

```rust
use xenora_audio_dsp::{Channels, Encoding, Format, Limits, Session};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Format::new(48_000, Channels::Stereo)?;
    let source = Format::new(48_000, Channels::Mono)?;
    let session = Session::new(output, Limits::default())?;
    let stream = session.open_stream(source)?;
    session.submit_pcm(stream, Encoding::S16Le, &[0, 0x40, 0, 0xc0])?;
    let mut pcm = [0.0; 4];
    session.renderer()?.render(&mut pcm)?;
    assert_eq!(pcm, [0.5, 0.5, -0.5, -0.5]);
    Ok(())
}
```

To ingest Gate 6 audio, make a DSP stream with the same rate and channel count
and call `xenora::transfer_gate6(&mut gate6_audio, &session, stream, 256)`.
That function only drains frames after the DSP accepts them. Call it again
after rendering if the destination has no free frames.

An original trusted guest fixture can call
`xenora::register_fixture_services(&mut registry, &session)` from its host
setup. The host supplies the guest with the raw handle for the fixture's
stream. The [API coverage report](API-COVERAGE.md) describes the exact
synthetic call contract.

## Local verification

The standalone experiment was checked on Apple M5 Pro, macOS 27.2.1,
Rust 1.100.0 and Xcode 27.2. The release example was a native arm64 Mach-O
linked to AudioToolbox and libSystem.

| Check | Observed result |
| --- | --- |
| ARM64 DSP and adapter tests | 15 passed, including 100 session lifecycle cycles |
| Existing ARM64 Xenora workspace tests | 486 passed, 22 ignored |
| Eight streams, 48 kHz, 256-frame blocks | 9.13 µs median and 9.42 µs p95 to submit and mix; 3.63 µs median for mixing and command retirement alone |
| Process memory in the offline benchmark | 6,471,680 bytes maximum resident set, with 278,528 bytes reserved for PCM at a fully staged block |
| Default CoreAudio output, 48 kHz, 256-frame buffers | 47 buffer returns, 12,032 DSP frames submitted, zero DSP underrun frames or queue timeline discontinuities |
| Timing on that CoreAudio run | 83.11 ms to first worker-observed buffer return from the start call, and a sampled queue lead of 1,622 frames (33.8 ms) |

The device measurement includes startup and worker scheduling. Queue lead is
an estimate on the CoreAudio sample timeline. Neither value measures acoustic
latency. Three buffers carry 768 initial silence frames (16 ms at 48 kHz) so
device startup failure leaves the DSP timeline and PCM unchanged.

See the [architecture](ARCHITECTURE.md) for ownership, limits, scheduling and
remaining work. This is a local AI-assisted experiment with independent
agent review of the resampling expectations and Audio Queue ownership.
Maintainer ownership review and human acceptance remain pending.
