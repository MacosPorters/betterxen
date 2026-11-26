//! Live run against a real D3DMetal.framework. Ignored by default because it
//! needs a user-supplied framework and a probe built with
//! `probe/build-probe.sh`. Run it explicitly:
//!
//! ```sh
//! BX_D3DMETAL_PROBE=target/d3dmetal-harness/bx-d3dmetal-probe \
//!   cargo test --locked -p d3dmetal-harness --test live_d3dmetal -- --ignored
//! ```
//!
//! A relative `BX_D3DMETAL_PROBE` is resolved against the workspace root.
//! When requested, missing prerequisites fail the test: an unavailable
//! framework or probe is never reported as a pass.

use std::path::Path;
use std::time::Duration;

use d3dmetal_harness::framework::{self, Discovery};
use d3dmetal_harness::{Verdict, evaluate, kernel};

const WORKSPACE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

#[test]
#[ignore = "needs a user-supplied D3DMetal.framework and BX_D3DMETAL_PROBE"]
fn sm_6_6_compute_readback_matches_host_values() {
    let probe = std::env::var_os("BX_D3DMETAL_PROBE")
        .map(|p| Path::new(WORKSPACE_ROOT).join(p))
        .expect("set BX_D3DMETAL_PROBE to a probe built by probe/build-probe.sh");
    let fw = match framework::discover_on_host(None) {
        Discovery::Found(fw) => fw,
        other => panic!("no usable D3DMetal.framework: {other:?}"),
    };
    let eval = evaluate(&probe, &fw, Duration::from_secs(60)).expect("probe ran");
    assert_eq!(
        eval.verdict,
        Verdict::Pass,
        "{}\n{}",
        eval.verdict,
        eval.stderr
    );
    assert_eq!(
        eval.report.readback.as_ref().map(Vec::len),
        Some(kernel::WORDS)
    );
    assert!(eval.report.shader_model >= Some(0x66));
}
