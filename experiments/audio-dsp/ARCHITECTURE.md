# Architecture

```text
Host PCM ───────────────┐
Gate 6 AudioSubsystem ──┼─> Session staging ─> FIFO commands ─> per-stream PCM
Synthetic guest ABI ────┘                                      │
                                                               v
                                               rate conversion, gain, mix
                                                               │
                                                               v
                                         offline read or Audio Queue worker
```

A clone of a `Session` shares one bounded state and output clock. The host opens
streams using validated channel layouts and rates. Handles contain a slot and a
generation unique within the process. Session ownership is enforced by exact
matches in the session's tables. Slots allocated again get new handles. A staged
PCM buffer belongs to one stream. Successful submission makes the buffer
in-flight until the renderer transfers its samples into that stream's queue and
retires the handle. The renderer applies pending commands in ticket order at
the beginning of each block. Closing a stream retires its staged buffers and
drops its pending commands. Closing a session invalidates all handles.

There is one active renderer per session. Relative to its clock, a stream can
start or resume at an exact output frame and can be paused without consuming
source samples. Gains apply per output channel. Mono duplicates into stereo.
Stereo downmix is the arithmetic mean. The mixer accumulates in FP64 and clips
once to normalized FP32. Linear interpolation tracks input phase as an integer
ratio of rates across input and output boundaries. On shortage it inserts
silence and holds the remaining source state. Pause and future starts do not
count as underruns. The renderer never waits for input.

Hard limits bound the stream, source-frame, buffer and command counts per
session. PCM allocations and destination capacity are checked before admission.
A failed admission leaves existing audio in place. The DSP serializes its
state with a mutex. Decoding, copying and retiring PCM can allocate on producer
or worker threads. The CoreAudio callback only identifies a reclaimed buffer
and updates atomics. The worker refills three platform-owned buffers and uses
synchronous queue disposal off the callback thread. If disposal fails, it
returns the status and retains callback context for potential late callbacks.

Audio Queue startup queries hardware support through the system framework at
runtime. If the default device cannot open or start at the requested format,
the call returns the platform error. The worker starts with three silence
buffers so startup can fail without consuming source PCM. It currently polls
at 1 ms and requires at least four milliseconds of audio per buffer. Timing
metrics include observed buffer returns, periodic queue-clock samples and
discontinuities. A discontinuity invalidates the queue lead estimate for
that playback session. Stopping flushes the device queue. A device error after
rendering has begun can discard an already mixed block.

## Integration boundary

The optional adapter uses Xenora's existing typed argument reader, import
registry and checked guest memory. The Gate 6 bridge preserves its producer,
PCM storage and existing format IDs. It submits through the same host API as
other PCM clients. It handles a finite batch based on capacity and preserves
source bytes when a submission fails. It does not implicitly replace the
existing application audio path.

## Current limits

- Interleaved S16LE and normalized finite F32LE input. Interleaved FP32 output.
- Mono and stereo at 8 through 192 kHz. No surround, effects graph, codecs,
  device selection, hot-plug recovery or drift compensation.
- The linear resampler does not implement anti-alias filtering for downsampling.
- Native output was tested on one M5 Pro. Other Apple Silicon devices depend on
  runtime Audio Queue support and remain unmeasured in this experiment.
- The standalone crate does not add shared app, runtime or FFI changes. Wiring
  the bridge into Gate 6's app session and native device lifetime requires
  coordination with the Gate 6 and runtime owners. Xenora #284 and #293 identify
  the overlapping FFI work.
