use std::collections::HashSet;

use iblt_set_reconciliation::dataset::{MAX_SET_SIZE, generate_sets, verify, verify_partial};
use iblt_set_reconciliation::{Config, DecodeReport, DecodeStatus, Iblt};

#[test]
fn generation_has_exact_disjoint_differences_and_is_reproducible() {
    let sets = generate_sets(100, 5, 7, u64::MAX).unwrap();
    let replay = generate_sets(100, 5, 7, u64::MAX).unwrap();
    assert_eq!(sets.a.len(), 105);
    assert_eq!(sets.b.len(), 107);
    assert_eq!(sets.a.intersection(&sets.b).count(), 100);
    assert_eq!(sets.only_a.len(), 5);
    assert_eq!(sets.only_b.len(), 7);
    assert_eq!(sets.a, replay.a);
    assert_eq!(sets.b, replay.b);
    assert!(sets.only_a.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(sets.only_b.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        sets.a.difference(&sets.b).copied().collect::<HashSet<_>>(),
        sets.only_a.iter().copied().collect()
    );
    assert_eq!(
        sets.b.difference(&sets.a).copied().collect::<HashSet<_>>(),
        sets.only_b.iter().copied().collect()
    );
}

#[test]
fn generation_accepts_boundary_sizes_and_rejects_oversized_sets() {
    let sets = generate_sets(MAX_SET_SIZE - 25, 25, 25, 42).unwrap();
    assert_eq!(sets.a.len(), MAX_SET_SIZE);
    assert_eq!(sets.b.len(), MAX_SET_SIZE);
    assert_eq!(sets.only_a.len() + sets.only_b.len(), 50);
    assert!(generate_sets(MAX_SET_SIZE, 1, 0, 42).is_err());
    assert!(generate_sets(usize::MAX, 1, 0, 42).is_err());
    let empty = generate_sets(0, 0, 0, 0).unwrap();
    assert!(empty.a.is_empty() && empty.b.is_empty());
}

#[test]
fn subtracting_full_tables_equals_building_only_differences() {
    for seed in [0, 1, 42, u64::MAX] {
        let config = Config::new(60, seed).unwrap();
        let sets = generate_sets(1_000, 7, 9, 22).unwrap();
        let full = Iblt::from_set(config, &sets.a)
            .unwrap()
            .subtract(&Iblt::from_set(config, &sets.b).unwrap())
            .unwrap();
        let only_a = sets.only_a.iter().copied().collect();
        let only_b = sets.only_b.iter().copied().collect();
        let direct = Iblt::from_set(config, &only_a)
            .unwrap()
            .subtract(&Iblt::from_set(config, &only_b).unwrap())
            .unwrap();
        assert_eq!(full.cells(), direct.cells());
    }
}

#[test]
fn oracle_rejects_incomplete_and_duplicate_output() {
    let sets = generate_sets(4, 2, 1, 31).unwrap();
    let mut report = DecodeReport {
        status: DecodeStatus::Complete,
        only_a: sets.only_a.clone(),
        only_b: sets.only_b.clone(),
        peeling_steps: 3,
        residual_cells: 0,
    };
    assert!(verify(&report, &sets));
    assert!(verify_partial(&report, &sets));
    report.status = DecodeStatus::Incomplete;
    assert!(!verify(&report, &sets));
    report.only_a.pop();
    assert!(verify_partial(&report, &sets));
    report.only_a.push(sets.only_b[0]);
    assert!(!verify_partial(&report, &sets));
    report.status = DecodeStatus::Complete;
    report.only_a = sets.only_a.clone();
    report.only_a[1] = report.only_a[0];
    assert!(!verify(&report, &sets));
    assert!(!verify_partial(&report, &sets));
}
