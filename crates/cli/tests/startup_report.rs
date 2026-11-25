// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::Command;

const STARTUP_STAGES: &[&str] = &[
    "parsed",
    "mapped",
    "relocated",
    "imports_bound",
    "tls_completed",
    "entry",
];

#[test]
fn trusted_fixture_preserves_step_failure() {
    let report = run_fixture("1", false);
    assert!(report.contains(r#""outcome":"stage_failure""#), "{report}");
    assert!(
        report.contains(r#""last_completed_stage":"entry""#),
        "{report}"
    );
    assert!(report.contains("step budget exhausted"), "{report}");
    assert_stages(&report, STARTUP_STAGES);
    assert!(!report.contains(r#""stage":"exit""#), "{report}");
}

#[test]
#[ignore = "requires a native Metal device with Apple family 7 capabilities"]
fn trusted_fixture_reports_verified_progress() {
    let report = run_fixture("100", true);
    assert!(report.contains(r#""outcome":"verified""#), "{report}");
    assert!(
        report.contains(r#""last_completed_stage":"framebuffer""#),
        "{report}"
    );
    let stages = [STARTUP_STAGES, &["exit", "framebuffer"]].concat();
    assert_stages(&report, &stages);
}

fn run_fixture(steps: &str, success: bool) -> String {
    let path = std::env::temp_dir().join(format!(
        "betterxen-report-test-{}-{steps}.json",
        std::process::id()
    ));
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/pe/trusted-runner.exe"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_betterxen"))
        .args([
            "run",
            "--experimental",
            "--pe",
            fixture,
            "--max-steps",
            steps,
            "--startup-report",
        ])
        .arg(&path)
        .output()
        .unwrap();
    let report = std::fs::read_to_string(&path);
    if report.is_ok() {
        std::fs::remove_file(&path).unwrap();
    }
    assert_eq!(output.status.success(), success, "{output:?}, {report:?}");
    report.unwrap()
}

fn assert_stages(report: &str, stages: &[&str]) {
    assert!(report.contains(r#""schema_version":1"#), "{report}");
    let mut remaining = report;
    for stage in stages {
        remaining = remaining
            .split_once(&format!(r#""stage":"{stage}""#))
            .unwrap()
            .1;
    }
}

#[test]
fn existing_report_path_prevents_worker_launch() {
    let path = std::env::temp_dir().join(format!(
        "betterxen-report-existing-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, "previous report").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_betterxen"))
        .args([
            "run",
            "--experimental",
            "--pe",
            "missing-fixture",
            "--startup-report",
        ])
        .arg(&path)
        .output()
        .unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(contents, "previous report");
    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(
        stderr.contains("could not create startup report"),
        "{stderr}"
    );
    assert!(!stderr.contains("No such file or directory"), "{stderr}");
}
