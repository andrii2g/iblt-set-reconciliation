use iblt_set_reconciliation::{Cell, Config, Iblt, encoding};
use std::collections::HashSet;

#[test]
fn wire_format_round_trips_signed_cells_and_has_exact_size() {
    let config = Config::new(6, 0x1122334455667788).unwrap();
    let a = Iblt::from_set(config, &HashSet::from([0, 19, u64::MAX])).unwrap();
    let b = Iblt::from_set(config, &HashSet::from([7, 19])).unwrap();
    let delta = a.subtract(&b).unwrap();
    let bytes = encoding::encode(&delta);
    assert_eq!(bytes.len(), 24 + 6 * 20);
    assert_eq!(&bytes[..8], b"IBLT\x01\x00\x01\x00");
    assert_eq!(&bytes[8..16], &6u64.to_le_bytes());
    assert_eq!(&bytes[16..24], &config.seed.to_le_bytes());
    let decoded = encoding::decode(&bytes).unwrap();
    assert_eq!(decoded.config(), config);
    assert_eq!(decoded.cells(), delta.cells());
}

#[test]
fn rejects_truncation_trailing_data_and_unknown_versions() {
    let table = Iblt::new(Config::new(3, 42).unwrap()).unwrap();
    let bytes = encoding::encode(&table);
    for end in 0..bytes.len() {
        assert!(encoding::decode(&bytes[..end]).is_err());
    }
    for offset in [0, 4, 6] {
        let mut invalid = bytes.clone();
        invalid[offset] ^= 0xff;
        assert!(encoding::decode(&invalid).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(encoding::decode(&trailing).is_err());
    let mut oversized = bytes.clone();
    oversized[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(encoding::decode(&oversized).is_err());
}

#[test]
fn count_encoding_is_signed_little_endian() {
    let mut cells = vec![Cell::default(); 3];
    cells[0].count = -1;
    let table = Iblt::from_cells(Config::new(3, 0).unwrap(), cells).unwrap();
    assert_eq!(&encoding::encode(&table)[24..28], &(-1i32).to_le_bytes());
}
