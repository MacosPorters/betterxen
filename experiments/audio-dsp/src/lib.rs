// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded interleaved PCM mixing, independent of the guest and output device.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod coreaudio;
#[cfg(feature = "xenora")]
pub mod xenora;

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

type Frame = [f32; 2];
type Result<T> = std::result::Result<T, AudioError>;
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioError {
    InvalidFormat,
    InvalidLength,
    InvalidSample,
    InvalidGain,
    InvalidLimits,
    InvalidHandle,
    WrongStream,
    Closed,
    RendererBusy,
    QueueFull,
    BufferBudget,
    StreamLimit,
    BufferInFlight,
    ClockOverflow,
    Poisoned,
}

impl fmt::Display for AudioError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self {
            Self::InvalidFormat => "unsupported PCM format",
            Self::InvalidLength => "invalid PCM or render length",
            Self::InvalidSample => "PCM sample is not finite or is outside the normalized range",
            Self::InvalidGain => "invalid channel gain",
            Self::InvalidLimits => "invalid DSP resource limits",
            Self::InvalidHandle => "invalid, stale, or foreign audio handle",
            Self::WrongStream => "audio buffer belongs to a different stream",
            Self::Closed => "audio session is closed",
            Self::RendererBusy => "audio session already has a renderer",
            Self::QueueFull => "audio command queue is full",
            Self::BufferBudget => "audio buffer budget exhausted",
            Self::StreamLimit => "audio session stream limit reached",
            Self::BufferInFlight => "audio buffer is queued for consumption",
            Self::ClockOverflow => "audio timeline or handle generation exhausted",
            Self::Poisoned => "audio session lock is poisoned",
        };
        out.write_str(detail)
    }
}

impl std::error::Error for AudioError {}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono = 1,
    Stereo = 2,
}

impl Channels {
    pub const fn count(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    rate: u32,
    channels: Channels,
}

impl Format {
    pub fn new(rate: u32, channels: Channels) -> Result<Self> {
        if !(8_000..=192_000).contains(&rate) {
            return Err(AudioError::InvalidFormat);
        }
        Ok(Self { rate, channels })
    }

    pub const fn rate(self) -> u32 {
        self.rate
    }

    pub const fn channels(self) -> Channels {
        self.channels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    S16Le,
    F32Le,
}

impl Encoding {
    pub const fn frame_bytes(self, channels: Channels) -> usize {
        let sample_bytes = match self {
            Self::S16Le => 2,
            Self::F32Le => 4,
        };
        sample_bytes * channels.count()
    }

    fn frames(self, channels: Channels, bytes: &[u8]) -> impl Iterator<Item = Result<Frame>> {
        bytes
            .chunks_exact(self.frame_bytes(channels))
            .map(move |chunk| {
                let sample = |index: usize| match self {
                    Self::S16Le => {
                        let offset = index * 2;
                        Ok(
                            f32::from(i16::from_le_bytes([chunk[offset], chunk[offset + 1]]))
                                / 32768.0,
                        )
                    }
                    Self::F32Le => {
                        let offset = index * 4;
                        let value =
                            f32::from_le_bytes(chunk[offset..offset + 4].try_into().unwrap());
                        if (-1.0..=1.0).contains(&value) {
                            Ok(value)
                        } else {
                            Err(AudioError::InvalidSample)
                        }
                    }
                };
                let left = sample(0)?;
                let right = if channels == Channels::Stereo {
                    sample(1)?
                } else {
                    left
                };
                Ok([left, right])
            })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub streams: usize,
    pub frames_per_stream: usize,
    pub buffers: usize,
    pub staging_frames: usize,
    pub commands: usize,
    pub render_frames: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            streams: 8,
            frames_per_stream: 4096,
            buffers: 32,
            staging_frames: 8192,
            commands: 64,
            render_frames: 4096,
        }
    }
}

impl Limits {
    fn check(self) -> Result<()> {
        let valid = (1..=64).contains(&self.streams)
            && (1..=65_536).contains(&self.frames_per_stream)
            && self.streams * self.frames_per_stream <= 262_144
            && (1..=256).contains(&self.buffers)
            && (1..=262_144).contains(&self.staging_frames)
            && (1..=4096).contains(&self.commands)
            && (1..=8192).contains(&self.render_frames);
        valid.then_some(()).ok_or(AudioError::InvalidLimits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamHandle(u64);
impl StreamHandle {
    pub const fn raw(self) -> u64 {
        self.0
    }
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferHandle(u64);

struct Pool<T, const TAG: u64>(Vec<Option<(u64, T)>>);

impl<T, const TAG: u64> Pool<T, TAG> {
    fn new(capacity: usize) -> Self {
        Self(std::iter::repeat_with(|| None).take(capacity).collect())
    }

    fn insert(&mut self, value: T, full: AudioError) -> Result<u64> {
        let index = self.0.iter().position(Option::is_none).ok_or(full)?;
        let generation = NEXT_GENERATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |generation| {
                (generation <= (u64::MAX >> 9)).then_some(generation + 1)
            })
            .map_err(|_| AudioError::ClockOverflow)?;
        let handle = (generation << 9) | TAG | index as u64;
        self.0[index] = Some((handle, value));
        Ok(handle)
    }

    fn entry(&mut self, handle: u64) -> Result<&mut Option<(u64, T)>> {
        self.0
            .get_mut((handle & 255) as usize)
            .filter(|entry| entry.as_ref().is_some_and(|(id, _)| *id == handle))
            .ok_or(AudioError::InvalidHandle)
    }

    fn get(&mut self, handle: u64) -> Result<&mut T> {
        Ok(&mut self.entry(handle)?.as_mut().unwrap().1)
    }

    fn take(&mut self, handle: u64) -> Result<T> {
        Ok(self.entry(handle)?.take().unwrap().1)
    }

    fn clear(&mut self) {
        self.0.fill_with(|| None);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Control {
    Gain([f32; 2]),
    Mute(bool),
    /// Start or resume when the output clock is at least this frame.
    StartAt(u64),
    Pause,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub output_frames: u64,
    pub commands_applied: u64,
    pub commands_cancelled: u64,
    pub command_overruns: u64,
    pub buffer_overruns: u64,
    pub underrun_stream_frames: u64,
    pub last_applied: Option<u64>,
    pub live_streams: usize,
    pub live_buffers: usize,
    pub pending_commands: usize,
    pub buffered_source_frames: usize,
    pub reserved_sample_bytes: usize,
    pub peak_staging_frames: usize,
}

struct Stream {
    format: Format,
    samples: VecDeque<Frame>,
    reserved: usize,
    phase: u32,
    gain: Frame,
    muted: bool,
    start_at: Option<u64>,
}

impl Stream {
    fn next(&mut self, output_rate: u32) -> Option<Frame> {
        self.consume_whole_frames(output_rate);
        let first = *self.samples.front()?;
        let frame = if self.phase == 0 {
            first
        } else {
            let second = self.samples.get(1)?;
            let t = self.phase as f32 / output_rate as f32;
            std::array::from_fn(|channel| first[channel] + (second[channel] - first[channel]) * t)
        };
        self.phase += self.format.rate;
        self.consume_whole_frames(output_rate);
        Some(frame)
    }

    fn consume_whole_frames(&mut self, output_rate: u32) {
        let count = ((self.phase / output_rate) as usize).min(self.samples.len());
        if count > 0 {
            self.samples.drain(..count);
            self.phase -= count as u32 * output_rate;
        }
    }
}

struct Buffer {
    owner: StreamHandle,
    frames: Vec<Frame>,
    queued: bool,
}

enum Command {
    Control(Control),
    Buffer(BufferHandle),
}

struct Pending {
    ticket: u64,
    stream: StreamHandle,
    command: Command,
}

struct State {
    closed: bool,
    renderer: bool,
    output: Format,
    limits: Limits,
    streams: Pool<Stream, 0>,
    buffers: Pool<Buffer, 256>,
    staging_frames: usize,
    commands: VecDeque<Pending>,
    next_ticket: u64,
    metrics: Metrics,
}

impl State {
    fn reserve_command(&mut self) -> Result<u64> {
        if self.commands.len() == self.limits.commands {
            self.metrics.command_overruns = self.metrics.command_overruns.saturating_add(1);
            return Err(AudioError::QueueFull);
        }
        let ticket = self.next_ticket;
        self.next_ticket = ticket.checked_add(1).ok_or(AudioError::ClockOverflow)?;
        Ok(ticket)
    }

    fn take_buffer(&mut self, handle: BufferHandle) -> Result<Buffer> {
        let buffer = self.buffers.take(handle.0)?;
        self.staging_frames -= buffer.frames.len();
        Ok(buffer)
    }

    fn apply_commands(&mut self) -> Result<()> {
        while let Some(pending) = self.commands.pop_front() {
            match pending.command {
                Command::Control(control) => {
                    let stream = self.streams.get(pending.stream.0)?;
                    match control {
                        Control::Gain(gain) => stream.gain = gain,
                        Control::Mute(muted) => stream.muted = muted,
                        Control::StartAt(frame) => stream.start_at = Some(frame),
                        Control::Pause => stream.start_at = None,
                    }
                }
                Command::Buffer(handle) => {
                    let buffer = self.take_buffer(handle)?;
                    let stream = self.streams.get(pending.stream.0)?;
                    stream.reserved -= buffer.frames.len();
                    stream.samples.extend(buffer.frames);
                }
            }
            self.metrics.commands_applied = self.metrics.commands_applied.saturating_add(1);
            self.metrics.last_applied = Some(pending.ticket);
        }
        Ok(())
    }
}

/// A session has one output clock and up to one active renderer.
#[derive(Clone)]
pub struct Session(Arc<Mutex<State>>);

impl Session {
    pub fn new(output: Format, limits: Limits) -> Result<Self> {
        limits.check()?;
        Ok(Self(Arc::new(Mutex::new(State {
            closed: false,
            renderer: false,
            output,
            limits,
            streams: Pool::new(limits.streams),
            buffers: Pool::new(limits.buffers),
            staging_frames: 0,
            commands: VecDeque::with_capacity(limits.commands),
            next_ticket: 0,
            metrics: Metrics::default(),
        }))))
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.0.lock().map_err(|_| AudioError::Poisoned)
    }

    fn live(&self) -> Result<MutexGuard<'_, State>> {
        let state = self.lock()?;
        if state.closed {
            Err(AudioError::Closed)
        } else {
            Ok(state)
        }
    }

    pub fn output_format(&self) -> Result<Format> {
        Ok(self.live()?.output)
    }

    pub fn stream_format(&self, stream: StreamHandle) -> Result<Format> {
        Ok(self.live()?.streams.get(stream.0)?.format)
    }

    /// An upper bound for validation before guest memory is copied.
    pub fn max_pcm_bytes(&self, stream: StreamHandle, encoding: Encoding) -> Result<usize> {
        let mut state = self.live()?;
        let frames = state
            .limits
            .frames_per_stream
            .min(state.limits.staging_frames);
        let format = state.streams.get(stream.0)?.format;
        Ok(frames * encoding.frame_bytes(format.channels))
    }

    /// Available source frames at the time of the call. Other producers can consume this capacity.
    pub fn available_pcm_frames(&self, stream: StreamHandle) -> Result<usize> {
        let mut state = self.live()?;
        let staging = state.limits.staging_frames - state.staging_frames;
        let capacity = state.limits.frames_per_stream;
        let stream = state.streams.get(stream.0)?;
        Ok(staging.min(capacity - stream.samples.len() - stream.reserved))
    }

    pub fn open_stream(&self, format: Format) -> Result<StreamHandle> {
        let mut state = self.live()?;
        let capacity = state.limits.frames_per_stream;
        if capacity < 2 && !format.rate.is_multiple_of(state.output.rate) {
            return Err(AudioError::InvalidLimits);
        }
        if state.streams.0.iter().all(Option::is_some) {
            return Err(AudioError::StreamLimit);
        }
        state
            .streams
            .insert(
                Stream {
                    format,
                    samples: VecDeque::with_capacity(capacity),
                    reserved: 0,
                    phase: 0,
                    gain: [1.0; 2],
                    muted: false,
                    start_at: Some(0),
                },
                AudioError::StreamLimit,
            )
            .map(StreamHandle)
    }

    /// Cancel queued commands and retire every buffer owned by this stream.
    pub fn close_stream(&self, stream: StreamHandle) -> Result<()> {
        let mut state = self.live()?;
        state.streams.take(stream.0)?;
        let before = state.commands.len();
        state.commands.retain(|pending| pending.stream != stream);
        let cancelled = before - state.commands.len();
        state.metrics.commands_cancelled = state
            .metrics
            .commands_cancelled
            .saturating_add(cancelled as u64);
        let mut released = 0;
        for slot in &mut state.buffers.0 {
            if slot
                .as_ref()
                .is_some_and(|(_, buffer)| buffer.owner == stream)
            {
                released += slot.take().unwrap().1.frames.len();
            }
        }
        state.staging_frames -= released;
        Ok(())
    }

    pub fn close(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.closed = true;
        state.metrics.commands_cancelled = state
            .metrics
            .commands_cancelled
            .saturating_add(state.commands.len() as u64);
        state.commands.clear();
        state.streams.clear();
        state.buffers.clear();
        state.staging_frames = 0;
        Ok(())
    }

    pub fn allocate_pcm(
        &self,
        stream: StreamHandle,
        encoding: Encoding,
        bytes: &[u8],
    ) -> Result<BufferHandle> {
        let mut state = self.live()?;
        let channels = state.streams.get(stream.0)?.format.channels;
        let stride = encoding.frame_bytes(channels);
        if bytes.is_empty() || !bytes.len().is_multiple_of(stride) {
            return Err(AudioError::InvalidLength);
        }
        let count = bytes.len() / stride;
        if count > state.limits.frames_per_stream
            || count > state.limits.staging_frames - state.staging_frames
            || state.buffers.0.iter().all(Option::is_some)
        {
            state.metrics.buffer_overruns = state.metrics.buffer_overruns.saturating_add(1);
            return Err(AudioError::BufferBudget);
        }
        let frames = encoding
            .frames(channels, bytes)
            .collect::<Result<Vec<_>>>()?;
        let handle = state.buffers.insert(
            Buffer {
                owner: stream,
                frames,
                queued: false,
            },
            AudioError::BufferBudget,
        )?;
        state.staging_frames += count;
        state.metrics.peak_staging_frames =
            state.metrics.peak_staging_frames.max(state.staging_frames);
        Ok(BufferHandle(handle))
    }

    pub fn retire_buffer(&self, buffer: BufferHandle) -> Result<()> {
        let mut state = self.live()?;
        if state.buffers.get(buffer.0)?.queued {
            return Err(AudioError::BufferInFlight);
        }
        state.take_buffer(buffer)?;
        Ok(())
    }

    /// Transfer a staged PCM buffer to the queue. The next render retires the handle.
    pub fn submit_buffer(&self, stream: StreamHandle, buffer: BufferHandle) -> Result<u64> {
        let mut state = self.live()?;
        let buffer_slot = state.buffers.get(buffer.0)?;
        if buffer_slot.owner != stream {
            return Err(AudioError::WrongStream);
        }
        if buffer_slot.queued {
            return Err(AudioError::BufferInFlight);
        }
        let count = buffer_slot.frames.len();
        let limit = state.limits.frames_per_stream;
        let dest = state.streams.get(stream.0)?;
        if count > limit - dest.samples.len() - dest.reserved {
            state.metrics.buffer_overruns = state.metrics.buffer_overruns.saturating_add(1);
            return Err(AudioError::BufferBudget);
        }
        let ticket = state.reserve_command()?;
        state.streams.get(stream.0)?.reserved += count;
        state.buffers.get(buffer.0)?.queued = true;
        state.commands.push_back(Pending {
            ticket,
            stream,
            command: Command::Buffer(buffer),
        });
        Ok(ticket)
    }

    pub fn submit_pcm(
        &self,
        stream: StreamHandle,
        encoding: Encoding,
        bytes: &[u8],
    ) -> Result<u64> {
        let handle = self.allocate_pcm(stream, encoding, bytes)?;
        let result = self.submit_buffer(stream, handle);
        if result.is_err() {
            // A concurrent close may already have retired the allocation.
            let _ = self.retire_buffer(handle);
        }
        result
    }

    pub fn control(&self, stream: StreamHandle, control: Control) -> Result<u64> {
        if let Control::Gain(gain) = control
            && !gain.into_iter().all(|gain| (0.0..=4.0).contains(&gain))
        {
            return Err(AudioError::InvalidGain);
        }
        let mut state = self.live()?;
        state.streams.get(stream.0)?;
        let ticket = state.reserve_command()?;
        state.commands.push_back(Pending {
            ticket,
            stream,
            command: Command::Control(control),
        });
        Ok(ticket)
    }

    pub fn renderer(&self) -> Result<Renderer> {
        let mut state = self.live()?;
        if state.renderer {
            return Err(AudioError::RendererBusy);
        }
        state.renderer = true;
        Ok(Renderer(self.clone()))
    }

    pub fn metrics(&self) -> Result<Metrics> {
        let state = self.lock()?;
        let mut metrics = state.metrics;
        metrics.live_streams = state.streams.0.iter().flatten().count();
        metrics.live_buffers = state.buffers.0.iter().flatten().count();
        metrics.pending_commands = state.commands.len();
        metrics.buffered_source_frames = state.staging_frames
            + state
                .streams
                .0
                .iter()
                .flatten()
                .map(|(_, stream)| stream.samples.len())
                .sum::<usize>();
        metrics.reserved_sample_bytes = std::mem::size_of::<Frame>()
            * (state
                .streams
                .0
                .iter()
                .flatten()
                .map(|(_, stream)| stream.samples.capacity())
                .sum::<usize>()
                + state
                    .buffers
                    .0
                    .iter()
                    .flatten()
                    .map(|(_, buffer)| buffer.frames.capacity())
                    .sum::<usize>());
        Ok(metrics)
    }
}

/// Exclusive sink for deterministic offline output or native playback.
pub struct Renderer(Session);

impl Renderer {
    /// Apply pending commands in ticket order, then render consecutive interleaved FP32 frames.
    pub fn render(&mut self, out: &mut [f32]) -> Result<()> {
        let mut state = self.0.live()?;
        let output = state.output;
        let channels = output.channels.count();
        if out.is_empty()
            || !out.len().is_multiple_of(channels)
            || out.len() / channels > state.limits.render_frames
        {
            return Err(AudioError::InvalidLength);
        }
        let start = state.metrics.output_frames;
        let end = start
            .checked_add((out.len() / channels) as u64)
            .ok_or(AudioError::ClockOverflow)?;
        state.apply_commands()?;
        let mut underruns = 0;
        for (clock, dest) in (start..end).zip(out.chunks_exact_mut(channels)) {
            let mut mixed = [0.0f64; 2];
            for (_, stream) in state.streams.0.iter_mut().flatten() {
                if stream.start_at.is_none_or(|frame| frame > clock) {
                    continue;
                }
                let Some(frame) = stream.next(output.rate) else {
                    underruns += 1;
                    continue;
                };
                if stream.muted {
                    continue;
                }
                if channels == 1 {
                    mixed[0] += (f64::from(frame[0]) + f64::from(frame[1]))
                        * 0.5
                        * f64::from(stream.gain[0]);
                } else {
                    for channel in 0..2 {
                        mixed[channel] +=
                            f64::from(frame[channel]) * f64::from(stream.gain[channel]);
                    }
                }
            }
            for (dest, value) in dest.iter_mut().zip(mixed) {
                *dest = value.clamp(-1.0, 1.0) as f32;
            }
        }
        state.metrics.output_frames = end;
        state.metrics.underrun_stream_frames = state
            .metrics
            .underrun_stream_frames
            .saturating_add(underruns);
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.renderer = false;
        }
    }
}
