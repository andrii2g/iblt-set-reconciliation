use iblt_set_reconciliation::experiment::{SweepOptions, run_sweep, sweep_csv};

#[test]
fn sweep_counts_known_successes_and_stalls_without_false_completion() {
    let rows = run_sweep(&SweepOptions {
        differences: vec![0, 1, 2, 5],
        capacities: vec![3, 90],
        trials: 8,
        data_seed: 42,
        seed_start: 0,
    })
    .unwrap();
    assert_eq!(rows.len(), 8);
    for row in &rows {
        assert_eq!(row.successes + row.incomplete + row.false_completions, 8);
        assert_eq!(row.false_completions, 0);
        assert_eq!(row.partial_mismatches, 0);
        assert_eq!(row.data_seed, 42);
        assert_eq!(row.seed_start, 0);
        assert_eq!(row.summary_bytes, 24 + 20 * row.cells);
        assert_eq!(row.decoded_keys_total, row.peeling_steps_total);
        if row.differences <= 1 {
            assert_eq!(row.successes, 8);
        }
        if row.cells == 3 && row.differences >= 2 {
            assert_eq!(row.incomplete, 8);
            assert_eq!(row.decoded_keys_total, 0);
        }
    }
    let csv = sweep_csv(&rows);
    assert_eq!(csv.lines().count(), 9);
    assert!(csv.starts_with("differences,cells,trials,data_seed,seed_start,successes,incomplete,false_completions,partial_mismatches,"));
    assert!(csv.ends_with('\n'));
}

#[test]
fn sweep_rejects_invalid_inputs_before_running() {
    let baseline = SweepOptions {
        differences: vec![5],
        capacities: vec![12],
        trials: 1,
        data_seed: 0,
        seed_start: 0,
    };
    assert!(
        run_sweep(&SweepOptions {
            trials: 0,
            ..baseline.clone()
        })
        .is_err()
    );
    assert!(
        run_sweep(&SweepOptions {
            capacities: vec![4],
            ..baseline.clone()
        })
        .is_err()
    );
    assert!(
        run_sweep(&SweepOptions {
            differences: vec![],
            ..baseline.clone()
        })
        .is_err()
    );
    assert!(
        run_sweep(&SweepOptions {
            differences: vec![200_001],
            ..baseline.clone()
        })
        .is_err()
    );
    assert!(
        run_sweep(&SweepOptions {
            trials: 2,
            seed_start: u64::MAX,
            ..baseline
        })
        .is_err()
    );
}
