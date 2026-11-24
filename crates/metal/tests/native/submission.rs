// SPDX-License-Identifier: GPL-3.0-or-later

use super::{context, fixture, native};

const ENTRY: &str = "fixture";
const INPUT: [u32; 4] = [0, 1, 71, u32::MAX];
const LIMIT: usize = 16;

fn source() -> String {
    fixture("output[id] = input[id] * 3u + 7u;")
}

fn expected(input: &[u32]) -> Vec<u32> {
    input
        .iter()
        .map(|word| word.wrapping_mul(3).wrapping_add(7))
        .collect()
}

#[test]
fn sweep_keeps_resources_until_completion() {
    let metal = context();
    let source = source();
    let (a, gate) = native(metal.submit_words_gated(&source, ENTRY, &INPUT));
    assert_eq!(metal.in_flight(), 1);
    let b = native(metal.submit_words(&source, ENTRY, &INPUT));
    assert_eq!(metal.in_flight(), 2);
    gate.open();
    assert_eq!(native(metal.wait(a)).output, expected(&INPUT));
    assert_eq!(native(metal.wait(b)).output, expected(&INPUT));
    assert_eq!(metal.in_flight(), 0);
}

#[test]
fn batch_can_be_collected_in_either_order() {
    let metal = context();
    let source = source();
    for reverse in [false, true] {
        let mut batch = Vec::new();
        for seed in 0..8 {
            let input = INPUT.map(|word| word.wrapping_add(seed));
            batch.push((input, native(metal.submit_words(&source, ENTRY, &input))));
        }
        if reverse {
            batch.reverse();
        }
        for (input, submission) in batch {
            let output = native(metal.wait(submission)).output;
            assert_eq!(output, expected(&input));
            assert_eq!(output, native(metal.run_words(&source, ENTRY, &input)));
        }
        assert_eq!(metal.tracked(), 0);
    }
}

#[test]
fn full_queue_returns_in_flight_limit_error() {
    let metal = context();
    let source = source();
    let (first, gate) = native(metal.submit_words_gated(&source, ENTRY, &INPUT));
    let mut tokens = vec![first];
    for _ in 1..LIMIT {
        tokens.push(native(metal.submit_words(&source, ENTRY, &INPUT)));
    }
    assert_eq!(metal.in_flight(), LIMIT);
    let Err(error) = metal.submit_words(&source, ENTRY, &INPUT) else {
        panic!("submission past the in-flight limit succeeded");
    };
    assert_eq!(error.operation, "in-flight limit");
    gate.open();
    for token in tokens {
        assert_eq!(native(metal.wait(token)).output, expected(&INPUT));
    }
    assert_eq!(metal.in_flight(), 0);
}

#[test]
fn dropped_token_retires_after_completion() {
    let metal = context();
    let source = source();
    let (a, gate) = native(metal.submit_words_gated(&source, ENTRY, &INPUT));
    drop(a);
    let b = native(metal.submit_words(&source, ENTRY, &INPUT));
    assert_eq!(metal.in_flight(), 2);
    drop(gate);
    assert_eq!(native(metal.wait(b)).output, expected(&INPUT));
    assert_eq!(metal.in_flight(), 0);
    assert_eq!(metal.tracked(), 0);
}

#[test]
fn finished_result_survives_later_submission() {
    let metal = context();
    let source = source();
    let a = native(metal.submit_words(&source, ENTRY, &INPUT));
    let b = native(metal.submit_words(&source, ENTRY, &[9]));
    assert_eq!(native(metal.wait(b)).output, expected(&[9]));
    assert_eq!(metal.in_flight(), 0);
    assert_eq!(metal.tracked(), 1);
    let profile = native(metal.wait(a));
    assert_eq!(profile.output, expected(&INPUT));
    assert_eq!(profile.wait_seconds, 0.0);
    assert_eq!(metal.tracked(), 0);
}
