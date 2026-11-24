// SPDX-License-Identifier: GPL-3.0-or-later

//! Gate 3 acceptance on a native Metal device: persistent guest rendering through
//! the C interface. Needs `--features gpu-tests` (scripts/test-gpu.sh).
#![cfg(feature = "gpu-tests")]

use std::sync::Mutex;
use std::time::{Duration, Instant};
use xenora_ffi::*;

/// Tests share the process-wide session table and one GPU; run them one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// Expected frame `id`, derived only from the geometry: the guest's frame index picks
/// column `id % 4`, which is opaque red (x9-constant-ps exports (1, 0, 0, 1)); the
/// other columns keep the clear colour (19, 43, 93, 251).
fn expected(id: u32) -> Vec<u8> {
    let mut frame = Vec::new();
    for _row in 0..3 {
        for column in 0..4 {
            if column == id % 4 {
                frame.extend([255, 0, 0, 255]);
            } else {
                frame.extend([19, 43, 93, 251]);
            }
        }
    }
    frame
}

struct Status {
    state: u32,
    completed: u32,
    frame_id: u32,
    rgba: [u8; 48],
}

fn status(session: u64) -> Status {
    let mut s = Status {
        state: 99,
        completed: 0,
        frame_id: 0,
        rgba: [0; 48],
    };
    let code = unsafe {
        xenora_session_frame_loop_status(
            session,
            &mut s.state,
            &mut s.completed,
            &mut s.frame_id,
            s.rgba.as_mut_ptr(),
            48,
        )
    };
    assert_eq!(code, XENORA_OK);
    s
}

fn session() -> u64 {
    let mut session = 0;
    assert_eq!(
        unsafe { xenora_session_create(XENORA_ABI_VERSION, &mut session) },
        XENORA_OK
    );
    session
}

fn wait_until(session: u64, what: &str, done: impl Fn(&Status) -> bool) -> Status {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let s = status(session);
        if done(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what} (state {}, completed {})",
            s.state,
            s.completed
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Physical memory footprint of this process (macOS `proc_pid_rusage`, RUSAGE_INFO_V2).
fn footprint() -> u64 {
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut [u64; 32]) -> i32;
    }
    let mut info = [0u64; 32];
    assert_eq!(
        unsafe { proc_pid_rusage(std::process::id() as i32, 2, &mut info) },
        0
    );
    info[9] // after the 16-byte UUID: user, system, idle wakeups, interrupts, pageins, wired, resident, footprint
}

#[test]
fn three_hundred_guest_frames_arrive_verified_and_in_order() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let session = session();
    assert_eq!(xenora_session_start_frame_loop(session, 300), XENORA_OK);
    assert_eq!(
        xenora_session_start_frame_loop(session, 300),
        XENORA_INVALID_STATE
    );
    let s = wait_until(session, "300 frames", |s| s.state != XENORA_LOOP_RUNNING);
    assert_eq!(s.state, XENORA_LOOP_FINISHED);
    assert_eq!(s.completed, 300);
    assert_eq!(s.frame_id, 299);
    assert_eq!(s.rgba.to_vec(), expected(299));
    assert_eq!(xenora_session_destroy(session), XENORA_OK);
}

#[test]
fn observed_frame_ids_never_go_backwards_and_match_their_pixels() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let session = session();
    assert_eq!(xenora_session_start_frame_loop(session, 2_000), XENORA_OK);
    let mut previous = None;
    let mut samples = 0;
    loop {
        let s = status(session);
        if s.frame_id != XENORA_NO_FRAME {
            if let Some(p) = previous {
                assert!(s.frame_id >= p, "frame id went from {p} to {}", s.frame_id);
            }
            assert!(
                s.frame_id < s.completed,
                "id {} not below completed {}",
                s.frame_id,
                s.completed
            );
            assert_eq!(
                s.rgba.to_vec(),
                expected(s.frame_id),
                "frame {}",
                s.frame_id
            );
            previous = Some(s.frame_id);
            samples += 1;
        }
        if s.state != XENORA_LOOP_RUNNING {
            assert_eq!(s.state, XENORA_LOOP_FINISHED);
            assert_eq!((s.completed, s.frame_id), (2_000, 1_999));
            break;
        }
    }
    assert!(samples > 1);
    xenora_session_destroy(session);
}

#[test]
fn pause_holds_the_guest_resume_continues_and_stop_halts_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let session = session();
    assert_eq!(
        xenora_session_start_frame_loop(session, XENORA_FRAME_LOOP_MAX_FRAMES),
        XENORA_OK
    );
    wait_until(session, "10 frames", |s| s.completed >= 10);

    assert_eq!(xenora_session_pause_frame_loop(session), XENORA_OK);
    // At most the frame already being rendered can still complete after pausing.
    std::thread::sleep(Duration::from_millis(50));
    let held = status(session);
    std::thread::sleep(Duration::from_millis(200));
    let later = status(session);
    assert_eq!(later.state, XENORA_LOOP_PAUSED);
    assert_eq!(later.completed, held.completed, "no frames while paused");
    assert_eq!(later.frame_id, held.completed - 1);
    assert_eq!(later.rgba.to_vec(), expected(later.frame_id));

    assert_eq!(xenora_session_resume_frame_loop(session), XENORA_OK);
    let resumed = wait_until(session, "progress after resume", |s| {
        s.completed >= held.completed + 10
    });
    assert_eq!(resumed.state, XENORA_LOOP_RUNNING);

    assert_eq!(xenora_session_stop_frame_loop(session), XENORA_OK);
    let stopped = status(session);
    assert_eq!(stopped.state, XENORA_LOOP_STOPPED);
    assert!(stopped.completed < XENORA_FRAME_LOOP_MAX_FRAMES);
    assert_eq!(stopped.frame_id, stopped.completed - 1);
    assert_eq!(stopped.rgba.to_vec(), expected(stopped.frame_id));
    // Pause and resume are refused once the guest has halted.
    assert_eq!(
        xenora_session_pause_frame_loop(session),
        XENORA_INVALID_STATE
    );
    assert_eq!(
        xenora_session_resume_frame_loop(session),
        XENORA_INVALID_STATE
    );
    assert_eq!(xenora_session_destroy(session), XENORA_OK);
}

#[test]
fn destroying_a_running_or_paused_session_stops_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    for pause_first in [false, true] {
        let session = session();
        assert_eq!(
            xenora_session_start_frame_loop(session, XENORA_FRAME_LOOP_MAX_FRAMES),
            XENORA_OK
        );
        wait_until(session, "a frame", |s| s.completed >= 1);
        if pause_first {
            assert_eq!(xenora_session_pause_frame_loop(session), XENORA_OK);
        }
        let started = Instant::now();
        assert_eq!(xenora_session_destroy(session), XENORA_OK);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "destroy joined promptly"
        );
        let mut x = 0;
        let mut rgba = [0u8; 48];
        assert_eq!(
            unsafe {
                xenora_session_frame_loop_status(
                    session,
                    &mut x,
                    &mut x,
                    &mut x,
                    rgba.as_mut_ptr(),
                    48,
                )
            },
            XENORA_INVALID_HANDLE
        );
    }
}

#[test]
fn repeated_sessions_do_not_grow_the_memory_footprint() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let run = || {
        let session = session();
        assert_eq!(xenora_session_start_frame_loop(session, 300), XENORA_OK);
        let s = wait_until(session, "300 frames", |s| s.state != XENORA_LOOP_RUNNING);
        assert_eq!((s.state, s.completed), (XENORA_LOOP_FINISHED, 300));
        assert_eq!(xenora_session_destroy(session), XENORA_OK);
    };
    // Warm up so one-time allocations (Metal device, shader compiler) are excluded.
    run();
    run();
    let baseline = footprint();
    for _ in 0..5 {
        run();
    }
    let after = footprint();
    let growth = after.saturating_sub(baseline);
    println!("footprint baseline={baseline} after_5_sessions={after} growth={growth}");
    assert!(
        growth < 16 << 20,
        "footprint grew by {growth} bytes over 5 sessions of 300 frames"
    );
}

/// Regression test for #290: the footprint read right after each destroy must stay
/// flat across many sessions. Each session used to open its own Metal command queue,
/// and the GPU driver frees a queue's working set (about 45 MiB on an Apple M5 with
/// validation) asynchronously after release, so these readings swung by about 50 MiB
/// from session to session. Sessions now share one queue. The mix of sessions covers
/// normal completion, pause and resume, and destruction while running.
#[test]
fn fifty_sessions_keep_a_flat_footprint_after_each_destroy() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let one = |kind: usize| {
        let session = session();
        match kind {
            0 => {
                assert_eq!(xenora_session_start_frame_loop(session, 100), XENORA_OK);
                let s = wait_until(session, "100 frames", |s| s.state != XENORA_LOOP_RUNNING);
                assert_eq!((s.state, s.completed), (XENORA_LOOP_FINISHED, 100));
            }
            1 => {
                assert_eq!(xenora_session_start_frame_loop(session, 100), XENORA_OK);
                wait_until(session, "10 frames", |s| s.completed >= 10);
                assert_eq!(xenora_session_pause_frame_loop(session), XENORA_OK);
                assert_eq!(xenora_session_resume_frame_loop(session), XENORA_OK);
                let s = wait_until(session, "100 frames", |s| s.state != XENORA_LOOP_RUNNING);
                assert_eq!((s.state, s.completed), (XENORA_LOOP_FINISHED, 100));
            }
            _ => {
                assert_eq!(
                    xenora_session_start_frame_loop(session, XENORA_FRAME_LOOP_MAX_FRAMES),
                    XENORA_OK
                );
                wait_until(session, "a frame", |s| s.completed >= 1);
            }
        }
        assert_eq!(xenora_session_destroy(session), XENORA_OK);
        footprint()
    };
    one(0); // the first session allocates the shared queue's working set
    let readings: Vec<u64> = (0..50).map(|i| one(i % 3)).collect();
    let mib: Vec<u64> = readings.iter().map(|r| r >> 20).collect();
    println!("footprint after each destroy (MiB): {mib:?}");
    let median = |values: &[u64]| {
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    };
    // Sustained growth: the typical reading at the end against the start.
    let (start, end) = (median(&readings[..10]), median(&readings[40..]));
    assert!(
        end.saturating_sub(start) < 16 << 20,
        "footprint grew from {start} to {end} bytes (median of first and last ten sessions)"
    );
    // Per-session churn, the cause fixed in #290: with a queue per session, about half
    // of all readings sat some 45 MiB away from the median. With the shared queue,
    // a reading is still occasionally caught while the driver frees a batch of
    // per-frame render resources: graphics-tagged memory only, 16 to 44 MiB, gone
    // within about 100 ms, measured in 13 of 6,000 sessions (0.22%). Allowing two such
    // readings in 50 keeps a false failure near 1 in 5,000 runs while per-session
    // churn would exceed it every time.
    let centre = median(&readings);
    let outliers: Vec<u64> = readings
        .iter()
        .filter(|&&r| r.abs_diff(centre) >= 16 << 20)
        .map(|r| r >> 20)
        .collect();
    assert!(
        outliers.len() <= 2,
        "{} of 50 readings were 16 MiB or more from the median {} MiB: {outliers:?}",
        outliers.len(),
        centre >> 20
    );
}

/// Two sessions render at the same time through the shared Metal queue (#290). Each
/// must keep its own frame sequence and pixels, pausing one must not stop the other,
/// and destroying one mid-run must not disturb the other.
#[test]
fn concurrent_sessions_share_the_queue_but_stay_isolated() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let (a, b) = (session(), session());
    assert_eq!(xenora_session_start_frame_loop(a, 400), XENORA_OK);
    assert_eq!(xenora_session_start_frame_loop(b, 250), XENORA_OK);
    wait_until(a, "session A started", |s| s.completed >= 5);
    wait_until(b, "session B started", |s| s.completed >= 5);

    // Pausing A holds only A.
    assert_eq!(xenora_session_pause_frame_loop(a), XENORA_OK);
    let a_held = wait_until(a, "session A paused", |s| s.state == XENORA_LOOP_PAUSED);
    let b_before = status(b).completed;
    let b_moving = wait_until(b, "session B progress while A is paused", |s| {
        s.completed >= b_before + 20 || s.state != XENORA_LOOP_RUNNING
    });
    assert!(
        b_moving.completed > b_before,
        "B kept rendering while A was paused"
    );
    let a_still = status(a);
    assert_eq!(a_still.state, XENORA_LOOP_PAUSED);
    // At most the frame A was rendering when paused can still complete.
    assert!(a_still.completed <= a_held.completed + 1);
    assert_eq!(a_still.rgba.to_vec(), expected(a_still.frame_id));
    assert_eq!(xenora_session_resume_frame_loop(a), XENORA_OK);

    // Both finish with their own counts and their own final frames.
    let a_done = wait_until(a, "session A finished", |s| s.state != XENORA_LOOP_RUNNING);
    let b_done = wait_until(b, "session B finished", |s| s.state != XENORA_LOOP_RUNNING);
    assert_eq!(
        (a_done.state, a_done.completed, a_done.frame_id),
        (XENORA_LOOP_FINISHED, 400, 399)
    );
    assert_eq!(
        (b_done.state, b_done.completed, b_done.frame_id),
        (XENORA_LOOP_FINISHED, 250, 249)
    );
    assert_eq!(a_done.rgba.to_vec(), expected(399));
    assert_eq!(b_done.rgba.to_vec(), expected(249));
    assert_eq!(xenora_session_destroy(a), XENORA_OK);
    assert_eq!(xenora_session_destroy(b), XENORA_OK);

    // Destroying one running session leaves the other running to completion.
    let (c, d) = (session(), session());
    assert_eq!(
        xenora_session_start_frame_loop(c, XENORA_FRAME_LOOP_MAX_FRAMES),
        XENORA_OK
    );
    assert_eq!(xenora_session_start_frame_loop(d, 300), XENORA_OK);
    wait_until(c, "session C started", |s| s.completed >= 5);
    wait_until(d, "session D started", |s| s.completed >= 5);
    assert_eq!(xenora_session_destroy(c), XENORA_OK);
    let d_done = wait_until(d, "session D finished", |s| s.state != XENORA_LOOP_RUNNING);
    assert_eq!(
        (d_done.state, d_done.completed, d_done.frame_id),
        (XENORA_LOOP_FINISHED, 300, 299)
    );
    assert_eq!(d_done.rgba.to_vec(), expected(299));
    assert_eq!(xenora_session_destroy(d), XENORA_OK);
}
