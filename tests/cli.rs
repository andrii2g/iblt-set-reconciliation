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

#[test]
fn diagram_shows_an_incomplete_zero_count_residual() {
    let path = output_path("diagram").with_extension("svg");
    let output = binary()
        .args([
            "demo", "--common", "0", "--a-only", "1", "--b-only", "1", "--cells", "3", "--svg",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let svg = fs::read_to_string(&path).unwrap();
    assert!(svg.starts_with("<svg "));
    assert!(svg.contains("INCOMPLETE"));
    assert!(svg.contains("3 nonzero residual cells"));
    fs::remove_file(path).unwrap();
}

#[test]
fn sweep_exports_seeded_measurements_and_treats_honest_stalls_as_results() {
    let csv = output_path("sweep").with_extension("csv");
    let json = output_path("sweep");
    let output = binary()
        .args([
            "sweep",
            "--differences",
            "0,2,5",
            "--cells",
            "3,30",
            "--trials",
            "3",
            "--seed-start",
            "100",
            "--output",
        ])
        .arg(&csv)
        .arg("--json")
        .arg(&json)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&fs::read(&json).unwrap()).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[2]["differences"], 2);
    assert_eq!(rows[2]["cells"], 3);
    assert_eq!(rows[2]["incomplete"], 3);
    for row in rows {
        assert_eq!(row["seed_start"], 100);
        assert_eq!(row["data_seed"], 7);
        assert_eq!(row["false_completions"], 0);
        assert_eq!(row["partial_mismatches"], 0);
    }
    let csv_text = fs::read_to_string(&csv).unwrap();
    assert_eq!(csv_text.lines().count(), 7);
    assert!(csv_text.lines().next().unwrap().contains("seed_start"));
    fs::remove_file(csv).unwrap();
    fs::remove_file(json).unwrap();
}
