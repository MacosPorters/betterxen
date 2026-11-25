// SPDX-License-Identifier: GPL-3.0-or-later

//! Run the benchmark's report, comparison and percentile tests in normal CI.
//! Timing runs remain confined to the standalone benchmark entry point.

#[allow(dead_code)]
#[path = "../benches/dbt_bench.rs"]
mod benchmark;
