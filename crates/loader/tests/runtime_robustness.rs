// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable-toolchain replay of the `runtime_services` fuzz driver: seeded
//! pseudo-random call sequences against the standard service registry.

mod support;

use support::robustness::Driver;

/// xorshift64*, so every run uses the same inputs.
fn inputs(seed: u64, count: usize, length: usize) -> impl Iterator<Item = Vec<u8>> {
    let mut state = seed;
    (0..count).map(move |_| {
        (0..length)
            .map(|_| {
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8
            })
            .collect()
    })
}

#[test]
fn random_service_sequences_keep_memory_and_register_invariants() {
    let driver = Driver::new();
    assert!(driver.service_count() > 150, "{}", driver.service_count());
    for input in inputs(0x5EED_CAFE, 300, 1024) {
        driver.run(&input);
    }
    driver.run(&[]);
}
