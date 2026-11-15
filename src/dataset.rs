//! Reproducible, unique set generation and exact oracle verification.

use std::collections::HashSet;

use crate::{DecodeReport, DecodeStatus};

/// The demonstration supports at most this many keys in either input set.
pub const MAX_SET_SIZE: usize = 100_000;

/// Two generated sets and their exact, sorted directional differences.
#[derive(Debug)]
pub struct GeneratedSets {
    pub a: HashSet<u64>,
    pub b: HashSet<u64>,
    pub only_a: Vec<u64>,
    pub only_b: Vec<u64>,
}

/// Generate exactly `common` shared keys and disjoint directional differences.
///
/// SplitMix64's finalizer is a permutation of `u64`: consecutive input indices
/// therefore yield unique identifiers without rejection sampling. The wrapping
/// addition of `data_seed` preserves uniqueness, even for seeds near `u64::MAX`.
pub fn generate_sets(
    common: usize,
    a_only: usize,
    b_only: usize,
    data_seed: u64,
) -> Result<GeneratedSets, String> {
    let a_size = common
        .checked_add(a_only)
        .ok_or("set A size overflows usize")?;
    let b_size = common
        .checked_add(b_only)
        .ok_or("set B size overflows usize")?;
    if a_size > MAX_SET_SIZE || b_size > MAX_SET_SIZE {
        return Err(format!(
            "each generated set must contain at most {MAX_SET_SIZE} keys"
        ));
    }
    let total = common
        .checked_add(a_only)
        .and_then(|n| n.checked_add(b_only))
        .ok_or("total generated key count overflows usize")?;

    let mut a = HashSet::with_capacity(a_size);
    let mut b = HashSet::with_capacity(b_size);
    for index in 0..total {
        let key = generated_key(index as u64, data_seed);
        if index < common {
            a.insert(key);
            b.insert(key);
        } else if index < a_size {
            a.insert(key);
        } else {
            b.insert(key);
        }
    }
    // Use ordinary set subtraction as the independent correctness oracle,
    // rather than relying only on the generator's intended membership ranges.
    let mut only_a: Vec<_> = a.difference(&b).copied().collect();
    let mut only_b: Vec<_> = b.difference(&a).copied().collect();
    only_a.sort_unstable();
    only_b.sort_unstable();
    Ok(GeneratedSets {
        a,
        b,
        only_a,
        only_b,
    })
}

fn generated_key(index: u64, data_seed: u64) -> u64 {
    let mut value = index.wrapping_add(data_seed);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Require completion and exact equality with both `HashSet` differences.
///
/// Sorting also catches duplicate decoder outputs instead of treating them as
/// harmless set members. Incomplete output never passes this check.
pub fn verify(report: &DecodeReport, sets: &GeneratedSets) -> bool {
    if report.status != DecodeStatus::Complete {
        return false;
    }
    let mut recovered_a = report.only_a.clone();
    let mut recovered_b = report.only_b.clone();
    recovered_a.sort_unstable();
    recovered_b.sort_unstable();
    recovered_a == sets.only_a && recovered_b == sets.only_b
}

/// Check that every recovered key is correct for its direction and appears once.
///
/// This also applies to incomplete reports: an unresolved residual is expected
/// at small capacities, while returning an incorrect key is an oracle mismatch.
pub fn verify_partial(report: &DecodeReport, sets: &GeneratedSets) -> bool {
    fn is_subset(actual: &[u64], expected: &[u64]) -> bool {
        let mut sorted = actual.to_vec();
        sorted.sort_unstable();
        sorted.windows(2).all(|pair| pair[0] < pair[1])
            && sorted.iter().all(|key| expected.binary_search(key).is_ok())
    }
    is_subset(&report.only_a, &sets.only_a) && is_subset(&report.only_b, &sets.only_b)
}
