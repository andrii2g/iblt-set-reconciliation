//! Versioned, little-endian summary encoding; independent of Rust struct padding.

use crate::{Cell, Config, Iblt};
use std::{error::Error, fmt};

pub const HEADER_BYTES: usize = 24;
pub const CELL_BYTES: usize = 20;
const MAGIC: &[u8; 4] = b"IBLT";
const FORMAT_VERSION: u16 = 1;
const HASH_LAYOUT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingError(String);

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for EncodingError {}

/// Exact wire length, including the configuration header.
pub fn encoded_len(cells: usize) -> Option<usize> {
    cells.checked_mul(CELL_BYTES)?.checked_add(HEADER_BYTES)
}

/// Encode all cells, including empty ones. No compression or native-layout casts.
pub fn encode(table: &Iblt) -> Vec<u8> {
    let config = table.config();
    let mut bytes = Vec::with_capacity(encoded_len(config.cells).expect("allocated table fits"));
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&HASH_LAYOUT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(config.cells as u64).to_le_bytes());
    bytes.extend_from_slice(&config.seed.to_le_bytes());
    for cell in table.cells() {
        bytes.extend_from_slice(&cell.count.to_le_bytes());
        bytes.extend_from_slice(&cell.key_xor.to_le_bytes());
        bytes.extend_from_slice(&cell.checksum_xor.to_le_bytes());
    }
    bytes
}

/// Decode one complete summary; unknown versions, truncation, and trailing data fail.
pub fn decode(bytes: &[u8]) -> Result<Iblt, EncodingError> {
    let fail = |message: &str| EncodingError(message.into());
    if bytes.len() < HEADER_BYTES {
        return Err(fail("summary is shorter than the 24-byte header"));
    }
    if &bytes[..4] != MAGIC {
        return Err(fail("invalid IBLT magic"));
    }
    if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != FORMAT_VERSION {
        return Err(fail("unsupported summary format version"));
    }
    if u16::from_le_bytes(bytes[6..8].try_into().unwrap()) != HASH_LAYOUT_VERSION {
        return Err(fail("unsupported hash/layout version"));
    }
    let cells = usize::try_from(u64::from_le_bytes(bytes[8..16].try_into().unwrap()))
        .map_err(|_| fail("cell count does not fit this platform"))?;
    let seed = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let config = Config::new(cells, seed).map_err(|err| EncodingError(err.to_string()))?;
    if encoded_len(cells) != Some(bytes.len()) {
        return Err(fail("summary length does not match its cell count"));
    }
    let cells = bytes[HEADER_BYTES..]
        .chunks_exact(CELL_BYTES)
        .map(|chunk| Cell {
            count: i32::from_le_bytes(chunk[..4].try_into().unwrap()),
            key_xor: u64::from_le_bytes(chunk[4..12].try_into().unwrap()),
            checksum_xor: u64::from_le_bytes(chunk[12..20].try_into().unwrap()),
        })
        .collect();
    Iblt::from_cells(config, cells).map_err(|err| EncodingError(err.to_string()))
}
