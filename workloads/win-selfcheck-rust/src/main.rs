// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload (issue #364): a Rust Windows console program that checks itself.
// Exercises thread-local storage and threads, which the contributor notes in
// docs/research/upstream-integration-audit.md report as the failure point of the plain
// `-gnu` Windows target. Exit 0 means every check passed.
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

thread_local! { static TLS: Cell<u64> = const { Cell::new(0) }; }
static TOTAL: AtomicU64 = AtomicU64::new(0);

fn main() {
    println!("selfcheck-rust arch={} os={}", std::env::consts::ARCH, std::env::consts::OS);
    let handles: Vec<_> = (1..=4u64)
        .map(|n| {
            thread::spawn(move || {
                for _ in 0..1000 {
                    TLS.with(|c| c.set(c.get() + n));
                }
                let v = TLS.with(Cell::get);
                TOTAL.fetch_add(v, Ordering::SeqCst);
                v
            })
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        let got = h.join().expect("thread panicked");
        let want = 1000 * (i as u64 + 1);
        if got != want {
            println!("FAIL tls thread {i}: got {got}, want {want}");
            std::process::exit(1);
        }
    }
    if TOTAL.load(Ordering::SeqCst) != 10_000 {
        println!("FAIL total");
        std::process::exit(2);
    }
    if TLS.with(Cell::get) != 0 {
        println!("FAIL main-thread TLS was modified by other threads");
        std::process::exit(3);
    }
    println!("PASS");
}
