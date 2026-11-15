//! Subtractable summaries for reconciling sets of unique `u64` identifiers.
//!
//! A decoder reports completion only when every field in every residual cell is
//! zero. Checksums make recovery probabilistic: an application needing exact
//! verification must compare the decoded result with an independent oracle.
//!
//! ```
//! use std::collections::HashSet;
//! use iblt_set_reconciliation::{Config, DecodeStatus, Iblt};
//!
//! let config = Config::new(30, 42)?;
//! let a = Iblt::from_set(config, &HashSet::from([1, 2, 3]))?;
//! let b = Iblt::from_set(config, &HashSet::from([2, 3, 4]))?;
//! let report = a.subtract(&b)?.decode();
//! assert_eq!(report.status, DecodeStatus::Complete);
//! assert_eq!(report.only_a, [1]);
//! assert_eq!(report.only_b, [4]);
//! # Ok::<(), iblt_set_reconciliation::IbltError>(())
//! ```

pub mod dataset;
pub mod encoding;
pub mod experiment;
pub mod hash;
pub mod iblt;
pub mod svg;

pub use iblt::{Cell, Config, DecodeReport, DecodeStatus, DecodeTrace, Iblt, IbltError, PeelStep};
