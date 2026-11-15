//! Version 1 hashing and cell layout.
//!
//! Each BLAKE3 input is a fixed domain tag followed by the seed and key, each
//! encoded as eight little-endian bytes. The first eight digest bytes are read
//! as a little-endian `u64`. Locations select one cell in each of three equal
//! regions. These rules are independent of Rust's randomized `HashMap` hasher.

use crate::Config;

/// Version of the hash algorithm, input encoding, and domain tags.
pub const HASH_VERSION: u8 = 1;
/// Version of the layout with three disjoint, equal-sized regions.
pub const LAYOUT_VERSION: u8 = 1;

fn word(domain: &[u8], seed: u64, key: u64) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&seed.to_le_bytes());
    hasher.update(&key.to_le_bytes());
    let digest = hasher.finalize();
    u64::from_le_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("eight digest bytes"),
    )
}

/// The three distinct cell locations for a key and a validated configuration.
///
/// # Panics
/// Panics if `config.cells` is less than three or not divisible by three.
pub fn locations(config: Config, key: u64) -> [usize; 3] {
    assert!(config.cells >= 3 && config.cells.is_multiple_of(3));
    let region = config.cells / 3;
    let words = [
        word(b"iblt-set-reconciliation/v1/location/0\0", config.seed, key),
        word(b"iblt-set-reconciliation/v1/location/1\0", config.seed, key),
        word(b"iblt-set-reconciliation/v1/location/2\0", config.seed, key),
    ];
    std::array::from_fn(|index| index * region + (words[index] % region as u64) as usize)
}

/// Independent checksum domain for validating a recoverable key.
pub fn checksum(seed: u64, key: u64) -> u64 {
    word(b"iblt-set-reconciliation/v1/checksum\0", seed, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_one_hash_vector() {
        let config = Config::new(96, 42).unwrap();
        assert_eq!(
            (locations(config, 0), checksum(42, 0)),
            ([10, 63, 84], 9_229_798_338_198_487_916)
        );
    }
}
