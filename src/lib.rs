//! Subtractable summaries for reconciling sets of unique `u64` identifiers.
//!
//! A decoder reports completion only when every field in every residual cell is
//! zero. Checksums make recovery probabilistic: an application needing exact
//! verification must compare the decoded result with an independent oracle.

pub mod hash;
pub mod iblt;

pub use iblt::{Cell, Config, DecodeReport, DecodeStatus, DecodeTrace, Iblt, IbltError, PeelStep};
