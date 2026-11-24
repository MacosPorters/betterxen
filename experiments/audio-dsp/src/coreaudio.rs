// SPDX-License-Identifier: GPL-3.0-or-later

//! Native ARM64 Audio Queue output. Only the worker calls into the DSP.

use crate::{AudioError, Format, Renderer, Session};
use std::ffi::c_void;
use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BUFFER_COUNT: usize = 3;
const UNAVAILABLE: u64 = u64::MAX;

#[derive(Debug)]
pub enum PlaybackError {
    Dsp(AudioError),
    CoreAudio {
        operation: &'static str,
        status: i32,
    },
    InvalidBuffer,
    InvalidBlockSize,
    Thread(std::io::Error),
    WorkerStopped,
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dsp(err) => write!(out, "DSP playback: {err}"),
            Self::CoreAudio { operation, status } => {
                write!(out, "{operation} failed (OSStatus {status})")
            }
            Self::InvalidBuffer => out.write_str("CoreAudio returned an invalid PCM buffer"),
            Self::InvalidBlockSize => out.write_str("invalid CoreAudio block size"),
            Self::Thread(err) => write!(out, "could not start the audio worker: {err}"),
            Self::WorkerStopped => out.write_str("audio worker stopped unexpectedly"),
        }
    }
}

impl std::error::Error for PlaybackError {}
impl From<AudioError> for PlaybackError {
    fn from(error: AudioError) -> Self {
        Self::Dsp(error)
    }
}

type Result<T> = std::result::Result<T, PlaybackError>;

#[derive(Clone, Copy, Debug, Default)]
pub struct PlaybackMetrics {
    pub buffer_callbacks: u64,
    pub submitted_frames: u64,
    pub priming_frames: u64,
    pub queue_clock_frames: Option<u64>,
    pub queued_lead_estimate_frames: Option<u64>,
    pub time_to_first_return_observed: Option<Duration>,
    pub timeline_discontinuities: u64,
}

struct Shared {
    stop: AtomicBool,
    buffers: [AtomicPtr<QueueBuffer>; BUFFER_COUNT],
    returned: AtomicU32,
    callbacks: AtomicU64,
    submitted: AtomicU64,
    priming: AtomicU64,
    queue_clock: AtomicU64,
    lead: AtomicU64,
    first_return: AtomicU64,
    discontinuities: AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            stop: AtomicBool::new(false),
            buffers: std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut())),
            returned: AtomicU32::new(0),
            callbacks: AtomicU64::new(0),
            submitted: AtomicU64::new(0),
            priming: AtomicU64::new(0),
            queue_clock: AtomicU64::new(UNAVAILABLE),
            lead: AtomicU64::new(UNAVAILABLE),
            first_return: AtomicU64::new(UNAVAILABLE),
            discontinuities: AtomicU64::new(0),
        }
    }
}

impl Shared {
    fn metrics(&self) -> PlaybackMetrics {
        let read = |value: &AtomicU64| value.load(Ordering::Relaxed);
        let clock = read(&self.queue_clock);
        let first = read(&self.first_return);
        let lead = read(&self.lead);
        let submitted = read(&self.submitted);
        PlaybackMetrics {
            buffer_callbacks: read(&self.callbacks),
            submitted_frames: submitted,
            priming_frames: read(&self.priming),
            queue_clock_frames: (clock != UNAVAILABLE).then_some(clock),
            queued_lead_estimate_frames: (lead != UNAVAILABLE).then_some(lead),
            time_to_first_return_observed: (first != UNAVAILABLE)
                .then(|| Duration::from_nanos(first)),
            timeline_discontinuities: read(&self.discontinuities),
        }
    }
}

/// A native output queue opened at runtime with the session's format on the default device.
pub struct Playback {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl Playback {
    pub fn start(session: &Session, frames_per_buffer: usize) -> Result<Self> {
        let format = session.output_format()?;
        let min = (format.rate() as usize).div_ceil(250); // Four milliseconds per buffer.
        let max = session.live()?.limits.render_frames.min(4096);
        if !(min..=max).contains(&frames_per_buffer) {
            return Err(PlaybackError::InvalidBlockSize);
        }
        let renderer = session.renderer()?;
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("xenora-audio".into())
            .spawn(move || {
                play(
                    renderer,
                    format,
                    frames_per_buffer,
                    &worker_shared,
                    ready_tx,
                )
            })
            .map_err(PlaybackError::Thread)?;
        if ready_rx.recv().is_err() {
            worker.join().map_err(|_| PlaybackError::WorkerStopped)??;
            return Err(PlaybackError::WorkerStopped);
        }
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn metrics(&self) -> PlaybackMetrics {
        self.shared.metrics()
    }

    pub fn stop(mut self) -> Result<PlaybackMetrics> {
        self.finish()?;
        Ok(self.metrics())
    }

    fn finish(&mut self) -> Result<()> {
        self.shared.stop.store(true, Ordering::Release);
        match self.worker.take() {
            Some(worker) => worker.join().map_err(|_| PlaybackError::WorkerStopped)?,
            None => Ok(()),
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn play(
    mut renderer: Renderer,
    format: Format,
    frames: usize,
    shared: &Arc<Shared>,
    ready: mpsc::SyncSender<()>,
) -> Result<()> {
    let bytes_per_frame = (format.channels().count() * size_of::<f32>()) as u32;
    let description = StreamDescription {
        sample_rate: f64::from(format.rate()),
        format: u32::from_be_bytes(*b"lpcm"),
        flags: 1 | (1 << 3), // kLinearPCMFormatFlagIsFloat | kLinearPCMFormatFlagIsPacked
        bytes_per_packet: bytes_per_frame,
        frames_per_packet: 1,
        bytes_per_frame,
        channels_per_frame: format.channels().count() as u32,
        bits_per_channel: 32,
        reserved: 0,
    };
    let byte_count = frames as u32 * bytes_per_frame;
    let mut queue = NativeQueue::new(&description, shared)?;
    let mut buffers = [None; BUFFER_COUNT];
    let result = (|| {
        for (index, slot) in buffers.iter_mut().enumerate() {
            let buffer = queue.allocate(byte_count)?;
            shared.buffers[index].store(buffer.as_ptr(), Ordering::Release);
            *slot = Some(buffer);
        }
        // A failed allocation, enqueue or start must leave the source and DSP clock intact.
        for buffer in buffers.into_iter().flatten() {
            queue.enqueue(buffer, byte_count, None)?;
            shared.priming.fetch_add(frames as u64, Ordering::Relaxed);
        }
        let clock_start = Instant::now();
        status("AudioQueueStart", unsafe {
            AudioQueueStart(queue.raw(), ptr::null())
        })?;
        if ready.send(()).is_err() {
            return Ok(());
        }
        let mut next_clock_read = clock_start;
        while !shared.stop.load(Ordering::Acquire) {
            let returned = shared.returned.swap(0, Ordering::Acquire);
            if returned != 0 {
                let elapsed = clock_start
                    .elapsed()
                    .as_nanos()
                    .min(u128::from(UNAVAILABLE - 1)) as u64;
                let _ = shared.first_return.compare_exchange(
                    UNAVAILABLE,
                    elapsed,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                );
                for (index, slot) in buffers.iter().enumerate() {
                    if returned & (1 << index) != 0 {
                        queue.enqueue(
                            slot.ok_or(PlaybackError::InvalidBuffer)?,
                            byte_count,
                            Some(&mut renderer),
                        )?;
                        shared.submitted.fetch_add(frames as u64, Ordering::Relaxed);
                    }
                }
            }
            let now = Instant::now();
            if now >= next_clock_read {
                queue.observe_clock(shared);
                next_clock_read = now + Duration::from_millis(10);
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    })();
    shared.stop.store(true, Ordering::Release);
    let disposal = queue.dispose();
    result.and(disposal)
}

struct NativeQueue {
    queue: Option<NonNull<c_void>>,
    callback_context: *const Shared,
    timeline: *mut c_void,
}

impl NativeQueue {
    fn new(description: &StreamDescription, shared: &Arc<Shared>) -> Result<Self> {
        let context = Arc::into_raw(Arc::clone(shared));
        let mut queue = ptr::null_mut();
        // SAFETY: matching C ABI. The separately retained context survives until synchronous disposal.
        let result = unsafe {
            AudioQueueNewOutput(
                description,
                returned_buffer,
                context.cast_mut().cast(),
                ptr::null(),
                ptr::null(),
                0,
                &mut queue,
            )
        };
        let Some(queue) = NonNull::new(queue) else {
            // With no queue there is no callback registration to retire.
            unsafe { drop(Arc::from_raw(context)) };
            return status("AudioQueueNewOutput", result).and(Err(PlaybackError::InvalidBuffer));
        };
        let mut native = Self {
            queue: Some(queue),
            callback_context: context,
            timeline: ptr::null_mut(),
        };
        status("AudioQueueNewOutput", result)?;
        status("AudioQueueCreateTimeline", unsafe {
            AudioQueueCreateTimeline(native.raw(), &mut native.timeline)
        })?;
        if native.timeline.is_null() {
            return Err(PlaybackError::InvalidBuffer);
        }
        Ok(native)
    }

    fn raw(&self) -> *mut c_void {
        self.queue.unwrap().as_ptr()
    }

    fn allocate(&self, byte_count: u32) -> Result<NonNull<QueueBuffer>> {
        let mut out = ptr::null_mut();
        status("AudioQueueAllocateBuffer", unsafe {
            AudioQueueAllocateBuffer(self.raw(), byte_count, &mut out)
        })?;
        NonNull::new(out).ok_or(PlaybackError::InvalidBuffer)
    }

    fn enqueue(
        &self,
        mut buffer: NonNull<QueueBuffer>,
        byte_count: u32,
        renderer: Option<&mut Renderer>,
    ) -> Result<()> {
        // SAFETY: the worker owns a fresh buffer or has acquired a completion signal.
        let storage = unsafe { buffer.as_mut() };
        let data = storage.data.cast::<f32>();
        if storage.capacity < byte_count || data.is_null() || !data.is_aligned() {
            return Err(PlaybackError::InvalidBuffer);
        }
        let sample_count = byte_count as usize / size_of::<f32>();
        // SAFETY: retained queue buffer with checked alignment and complete interleaved frames.
        // Initialize storage before creating the Rust f32 slice.
        let samples = unsafe {
            data.write_bytes(0, sample_count);
            std::slice::from_raw_parts_mut(data, sample_count)
        };
        if let Some(renderer) = renderer {
            renderer.render(samples)?;
        }
        storage.byte_count = byte_count;
        status("AudioQueueEnqueueBuffer", unsafe {
            AudioQueueEnqueueBuffer(self.raw(), buffer.as_ptr(), 0, ptr::null())
        })
    }

    fn observe_clock(&self, shared: &Shared) {
        let mut stamp = TimeStamp::default();
        let mut discontinuity = 0;
        let status = unsafe {
            AudioQueueGetCurrentTime(self.raw(), self.timeline, &mut stamp, &mut discontinuity)
        };
        if discontinuity != 0 {
            shared.discontinuities.fetch_add(1, Ordering::Relaxed);
        }
        let clock = if status == 0
            && discontinuity == 0
            && stamp.flags & 1 != 0
            && stamp.sample_time.is_finite()
            && stamp.sample_time >= 0.0
        {
            stamp.sample_time as u64
        } else {
            UNAVAILABLE
        };
        shared.queue_clock.store(clock, Ordering::Relaxed);
        // Sample lead on the worker using the submission count from this observation.
        // After a discontinuity the original submission epoch is no longer reliable.
        let lead = if clock != UNAVAILABLE && shared.discontinuities.load(Ordering::Relaxed) == 0 {
            let submitted = shared
                .submitted
                .load(Ordering::Relaxed)
                .saturating_add(shared.priming.load(Ordering::Relaxed));
            submitted.saturating_sub(clock)
        } else {
            UNAVAILABLE
        };
        shared.lead.store(lead, Ordering::Relaxed);
    }

    fn dispose(&mut self) -> Result<()> {
        let Some(queue) = self.queue.take() else {
            return Ok(());
        };
        // SAFETY: called off the callback thread with no locks. On success no more callbacks run.
        let result = unsafe { AudioQueueDispose(queue.as_ptr(), 1) };
        if result == 0 {
            unsafe { drop(Arc::from_raw(self.callback_context)) };
        }
        // If disposal fails the callback registration may still be live, so retain the context.
        status("AudioQueueDispose", result)
    }
}

impl Drop for NativeQueue {
    fn drop(&mut self) {
        let _ = self.dispose();
    }
}

unsafe extern "C" fn returned_buffer(
    context: *mut c_void,
    _: *mut c_void,
    buffer: *mut QueueBuffer,
) {
    // SAFETY: Audio Queue holds the explicitly retained Arc pointer until synchronous disposal.
    let shared = unsafe { &*context.cast::<Shared>() };
    if shared.stop.load(Ordering::Acquire) {
        return;
    }
    for (index, slot) in shared.buffers.iter().enumerate() {
        if slot.load(Ordering::Acquire) == buffer {
            shared.callbacks.fetch_add(1, Ordering::Relaxed);
            shared.returned.fetch_or(1 << index, Ordering::Release);
            break;
        }
    }
}

fn status(operation: &'static str, code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(PlaybackError::CoreAudio {
            operation,
            status: code,
        })
    }
}

#[repr(C)]
struct StreamDescription {
    sample_rate: f64,
    format: u32,
    flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct QueueBuffer {
    capacity: u32,
    data: *mut c_void,
    byte_count: u32,
    user_data: *mut c_void,
    packet_capacity: u32,
    packets: *mut c_void,
    packet_count: u32,
}

#[repr(C)]
#[derive(Default)]
struct TimeStamp {
    sample_time: f64,
    host_time: u64,
    rate_scalar: f64,
    word_clock: u64,
    smpte: [u32; 6],
    flags: u32,
    reserved: u32,
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioQueueNewOutput(
        description: *const StreamDescription,
        callback: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut QueueBuffer),
        context: *mut c_void,
        run_loop: *const c_void,
        mode: *const c_void,
        flags: u32,
        out: *mut *mut c_void,
    ) -> i32;
    fn AudioQueueAllocateBuffer(queue: *mut c_void, size: u32, out: *mut *mut QueueBuffer) -> i32;
    fn AudioQueueEnqueueBuffer(
        queue: *mut c_void,
        buffer: *mut QueueBuffer,
        packet_count: u32,
        descriptions: *const c_void,
    ) -> i32;
    fn AudioQueueStart(queue: *mut c_void, at: *const TimeStamp) -> i32;
    fn AudioQueueCreateTimeline(queue: *mut c_void, timeline: *mut *mut c_void) -> i32;
    fn AudioQueueGetCurrentTime(
        queue: *mut c_void,
        timeline: *mut c_void,
        stamp: *mut TimeStamp,
        discontinuity: *mut u8,
    ) -> i32;
    fn AudioQueueDispose(queue: *mut c_void, immediate: u8) -> i32;
}
