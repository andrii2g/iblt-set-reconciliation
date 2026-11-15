use std::collections::HashSet;

use iblt_set_reconciliation::hash::{checksum, locations};
use iblt_set_reconciliation::{Cell, Config, DecodeStatus, Iblt, IbltError};

fn set(keys: &[u64]) -> HashSet<u64> {
    keys.iter().copied().collect()
}

fn difference(config: Config, a: &HashSet<u64>, b: &HashSet<u64>) -> Iblt {
    Iblt::from_set(config, a)
        .unwrap()
        .subtract(&Iblt::from_set(config, b).unwrap())
        .unwrap()
}

#[test]
fn validates_capacity_and_dimensions() {
    for cells in [0, 1, 2, 4, 8] {
        assert!(Config::new(cells, 0).is_err());
    }
    assert!(Iblt::new(Config { cells: 0, seed: 0 }).is_err());
    assert!(Iblt::from_cells(Config::new(3, 0).unwrap(), vec![]).is_err());
}

#[test]
fn empty_and_equal_sets_cancel_completely_even_when_heavily_loaded() {
    let config = Config::new(3, 7).unwrap();
    let empty = HashSet::new();
    assert_eq!(
        difference(config, &empty, &empty).decode().status,
        DecodeStatus::Complete
    );
    let keys: HashSet<_> = (0..10_000).collect();
    let residual = difference(config, &keys, &keys);
    assert!(residual.cells().iter().all(|cell| cell.is_zero()));
    let report = residual.decode();
    assert_eq!(report.status, DecodeStatus::Complete);
    assert!(report.only_a.is_empty() && report.only_b.is_empty());
    assert_eq!(report.peeling_steps, 0);
}

#[test]
fn zero_and_maximum_keys_are_valid_and_both_directions_are_recovered() {
    let config = Config::new(300, 42).unwrap();
    let a = set(&[0, 7, 99]);
    let b = set(&[7, u64::MAX]);
    let report = difference(config, &a, &b).decode();
    assert_eq!(report.status, DecodeStatus::Complete);
    assert_eq!(report.only_a, vec![0, 99]);
    assert_eq!(report.only_b, vec![u64::MAX]);
    assert_eq!(report.peeling_steps, 3);
    assert_eq!(report.residual_cells, 0);
    let reverse = difference(config, &b, &a).decode();
    assert_eq!(reverse.only_a, report.only_b);
    assert_eq!(reverse.only_b, report.only_a);
}

#[test]
fn rejects_different_capacities_and_seeds() {
    let table = Iblt::new(Config::new(6, 0).unwrap()).unwrap();
    for config in [Config::new(9, 0).unwrap(), Config::new(6, 1).unwrap()] {
        assert!(matches!(
            table.subtract(&Iblt::new(config).unwrap()),
            Err(IbltError::IncompatibleConfig { .. })
        ));
    }
}

#[test]
fn zero_count_cells_with_unresolved_xors_are_incomplete() {
    // Every key maps to the same three cells, so signed counts cancel while
    // the two distinct keys remain unresolved.
    let config = Config::new(3, 0).unwrap();
    let residual = difference(config, &set(&[1]), &set(&[2]));
    assert!(residual.cells().iter().all(|cell| cell.count == 0));
    let report = residual.decode();
    assert_eq!(report.status, DecodeStatus::Incomplete);
    assert_eq!(report.residual_cells, 3);
    assert_eq!(report.peeling_steps, 0);
}

#[test]
fn stale_queue_entries_do_not_decode_a_key_multiple_times() {
    let config = Config::new(3, 123).unwrap();
    let report = difference(config, &set(&[5]), &set(&[])).decode();
    assert_eq!(report.only_a, vec![5]);
    assert_eq!(report.peeling_steps, 1);
    assert_eq!(report.status, DecodeStatus::Complete);
}

#[test]
fn rejects_false_purity_and_wrong_location() {
    let config = Config::new(12, 3).unwrap();
    let key = 15;
    let indices = locations(config, key);
    let wrong_index = (0..config.cells)
        .find(|index| !indices.contains(index))
        .unwrap();
    let mut cells = vec![Cell::default(); config.cells];
    cells[wrong_index] = Cell {
        count: 1,
        key_xor: key,
        checksum_xor: checksum(config.seed, key),
    };
    let report = Iblt::from_cells(config, cells).unwrap().decode();
    assert!(report.only_a.is_empty());
    assert_eq!(report.status, DecodeStatus::Incomplete);
    let mut cells = vec![Cell::default(); config.cells];
    cells[indices[0]] = Cell {
        count: -1,
        key_xor: key,
        checksum_xor: checksum(config.seed, key) ^ 1,
    };
    assert!(
        Iblt::from_cells(config, cells)
            .unwrap()
            .decode()
            .only_b
            .is_empty()
    );
}

#[test]
fn updates_are_atomic_when_counts_overflow() {
    let config = Config::new(3, 0).unwrap();
    let cells = vec![
        Cell::default(),
        Cell {
            count: i32::MAX,
            ..Cell::default()
        },
        Cell::default(),
    ];
    let mut table = Iblt::from_cells(config, cells).unwrap();
    let unchanged = table.clone();
    assert_eq!(table.insert(7), Err(IbltError::CountOverflow));
    assert_eq!(table, unchanged);
    let negative = Iblt::from_cells(
        config,
        vec![
            Cell {
                count: -1,
                ..Cell::default()
            };
            3
        ],
    )
    .unwrap();
    assert_eq!(table.subtract(&negative), Err(IbltError::CountOverflow));
    assert_eq!(table, unchanged);
}

#[test]
fn malformed_counts_do_not_wrap_during_decoding() {
    let config = Config::new(3, 0).unwrap();
    let key = 5;
    let pure = Cell {
        count: 1,
        key_xor: key,
        checksum_xor: checksum(config.seed, key),
    };
    let cells = vec![
        pure,
        Cell {
            count: i32::MIN,
            ..Cell::default()
        },
        pure,
    ];
    let report = Iblt::from_cells(config, cells).unwrap().decode();
    assert_eq!(report.status, DecodeStatus::Incomplete);
    assert_eq!(report.peeling_steps, 0);
    assert_eq!(report.residual_cells, 3);
}

#[test]
fn locations_are_distinct_and_hashes_are_reproducible() {
    let config = Config::new(96, 42).unwrap();
    for key in [0, 1, 500, u64::MAX] {
        let indices = locations(config, key);
        assert!(indices[0] < 32);
        assert!((32..64).contains(&indices[1]));
        assert!((64..96).contains(&indices[2]));
        assert_eq!(indices, locations(config, key));
        assert_eq!(checksum(config.seed, key), checksum(config.seed, key));
    }
    let keys = set(&[0, 3, 18, u64::MAX]);
    assert_eq!(
        Iblt::from_set(config, &keys).unwrap(),
        Iblt::from_set(config, &keys).unwrap()
    );
}

#[test]
fn trace_records_a_peeling_cascade_and_matches_untraced_decode() {
    // Search a fixed finite sequence for a short example where peeling creates
    // a new recoverable cell; this avoids depending on incidental hash output.
    let a = set(&[1, 2, 3, 4, 5]);
    let b = set(&[6, 7]);
    let (table, report, trace) = (0..100)
        .find_map(|seed| {
            let table = difference(Config::new(18, seed).unwrap(), &a, &b);
            let (report, trace) = table.clone().decode_with_trace();
            (report.status == DecodeStatus::Complete
                && trace
                    .steps
                    .iter()
                    .any(|step| !step.newly_recoverable.is_empty()))
            .then_some((table, report, trace))
        })
        .expect("the fixed examples contain a peeling cascade");
    assert_eq!(table.clone().decode(), report);
    assert_eq!(trace.initial_cells, table.cells());
    assert!(trace.final_cells.iter().all(|cell| cell.is_zero()));
    assert_eq!(trace.steps.len(), 7);
    let mut replay = trace.initial_cells.clone();
    for step in &trace.steps {
        for (slot, &index) in step.affected_cells.iter().enumerate() {
            replay[index].count -= step.sign;
            replay[index].key_xor ^= step.key;
            replay[index].checksum_xor ^= checksum(trace.config.seed, step.key);
            assert_eq!(replay[index], step.cells_after[slot]);
        }
    }
    assert_eq!(replay, trace.final_cells);
}

#[test]
fn seeded_cases_match_exact_hashset_subtraction() {
    let mut complete_cases = 0;
    for seed in 0..80 {
        let config = Config::new(300, seed).unwrap();
        let common: HashSet<_> = (0..300).collect();
        let mut a = common.clone();
        let mut b = common;
        a.extend((0..25).map(|key| 1000 + key * 7 + seed * 100));
        b.extend((0..25).map(|key| 20000 + key * 11 + seed * 100));
        let mut expected_a: Vec<_> = a.difference(&b).copied().collect();
        let mut expected_b: Vec<_> = b.difference(&a).copied().collect();
        expected_a.sort_unstable();
        expected_b.sort_unstable();
        let report = difference(config, &a, &b).decode();
        assert!(report.only_a.iter().all(|key| expected_a.contains(key)));
        assert!(report.only_b.iter().all(|key| expected_b.contains(key)));
        if report.status == DecodeStatus::Complete {
            complete_cases += 1;
            assert_eq!(report.only_a, expected_a);
            assert_eq!(report.only_b, expected_b);
            assert_eq!(report.residual_cells, 0);
        } else {
            assert!(report.residual_cells > 0);
            assert!(report.peeling_steps < expected_a.len() + expected_b.len());
        }
    }
    assert!(complete_cases > 70);
    assert!(
        complete_cases < 80,
        "fixed seeds also exercise incomplete recovery"
    );
}
