use std::{fs, path::PathBuf, process::Command};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_iblt-reconcile"))
}

fn output_path(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
    fs::create_dir_all(&path).unwrap();
    path.join(format!("{name}-{}.json", std::process::id()))
}

#[test]
fn demo_reports_exact_recovery_and_real_wire_size() {
    let path = output_path("complete");
    let output = binary()
        .args([
            "demo", "--common", "30", "--a-only", "3", "--b-only", "2", "--cells", "300", "--json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(report["oracle_match"], true);
    assert_eq!(report["decoded_keys"], 5);
    assert_eq!(report["summary_bytes"], 24 + 300 * 20);
    assert_eq!(report["decode"]["residual_cells"], 0);
    fs::remove_file(path).unwrap();
}

#[test]
fn stalled_zero_count_cells_exit_incomplete() {
    let path = output_path("incomplete");
    let output = binary()
        .args([
            "demo", "--common", "0", "--a-only", "1", "--b-only", "1", "--cells", "3", "--json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(report["oracle_match"], false);
    assert_eq!(report["decoded_keys"], 0);
    assert_eq!(report["decode"]["residual_cells"], 3);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Recovery stalled"));
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_capacity_and_oversized_sets_fail_cleanly() {
    for args in [
        vec!["demo", "--cells", "4"],
        vec!["demo", "--common", "100000", "--a-only", "1"],
    ] {
        let output = binary().args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
