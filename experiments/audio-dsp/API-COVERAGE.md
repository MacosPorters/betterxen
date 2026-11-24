# Audio API coverage

Proprietary Xbox audio DSP APIs are unsupported; this experiment defines only its own fixture services below.

## Original Xenora fixture services

These Win64 calls are defined only for this experiment under
`xenora_audio_fixture.dll`. The host registers them for a single DSP session
and provides a stream handle to trusted synthetic guest code.

| Import | Arguments |
| --- | --- |
| `SubmitPcm` | RCX stream handle, RDX guest PCM address, R8 byte length, R9D encoding (1 for S16LE, 2 for F32LE) |
| `ScheduleStart` | RCX stream handle, RDX absolute DSP output-frame clock position |

The result in RAX is 0 for accepted, 1 for an invalid handle, 2 for invalid
input, 3 for exhausted capacity or 4 for session failure. An invalid guest
memory range raises Xenora's checked memory fault. PCM must use complete frames,
be appropriately aligned and fit the configured transfer limit. Acceptance
places a command into the FIFO and does not report completed playback.

## Public references

- [Microsoft GDK overview](https://learn.microsoft.com/en-us/gaming/gdk/_content/gc/intro/overviews/introduction)
- [Microsoft XAudio2 introduction](https://learn.microsoft.com/en-us/windows/win32/xaudio2/xaudio2-introduction)
- [Microsoft WAVEFORMATEX frame alignment](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex)
- [Microsoft x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention)
- [Apple Audio Queue Services](https://developer.apple.com/documentation/audiotoolbox/audio-queue-services)
- [Apple AudioQueueNewOutput](https://developer.apple.com/documentation/audiotoolbox/audioqueuenewoutput)
- [Apple AudioQueueGetCurrentTime](https://developer.apple.com/documentation/audiotoolbox/audioqueuegetcurrenttime)
