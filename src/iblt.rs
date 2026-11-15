use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::fmt;

use serde::Serialize;

use crate::hash::{checksum, locations};

/// A table's capacity and deterministic hash seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Config {
    pub cells: usize,
    pub seed: u64,
}

impl Config {
    pub fn new(cells: usize, seed: u64) -> Result<Self, IbltError> {
        let config = Self { cells, seed };
        config.validate()?;
        Ok(config)
    }

    fn validate(self) -> Result<(), IbltError> {
        if self.cells < 3 || !self.cells.is_multiple_of(3) {
            return Err(IbltError::InvalidCapacity { cells: self.cells });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Cell {
    pub count: i32,
    pub key_xor: u64,
    pub checksum_xor: u64,
}

impl Cell {
    /// A zero count alone is insufficient after table subtraction.
    pub fn is_zero(self) -> bool {
        self.count == 0 && self.key_xor == 0 && self.checksum_xor == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IbltError {
    InvalidCapacity { cells: usize },
    CellCountMismatch { expected: usize, actual: usize },
    IncompatibleConfig { left: Config, right: Config },
    CountOverflow,
}

impl fmt::Display for IbltError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity { cells } => write!(
                formatter,
                "capacity must be at least 3 and divisible by 3 (got {cells})"
            ),
            Self::CellCountMismatch { expected, actual } => write!(
                formatter,
                "expected {expected} cells for this configuration, got {actual}"
            ),
            Self::IncompatibleConfig { left, right } => write!(
                formatter,
                "cannot subtract incompatible configurations: {left:?} and {right:?}"
            ),
            Self::CountOverflow => formatter.write_str("signed cell count overflow"),
        }
    }
}

impl Error for IbltError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecodeStatus {
    Complete,
    Incomplete,
}

/// Recovered entries are sorted, including when recovery is incomplete.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DecodeReport {
    pub status: DecodeStatus,
    pub only_a: Vec<u64>,
    pub only_b: Vec<u64>,
    /// Number of successful key removals, not queue pops.
    pub peeling_steps: usize,
    pub residual_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PeelStep {
    pub key: u64,
    /// `+1` means A-only; `-1` means B-only.
    pub sign: i32,
    pub source_cell: usize,
    pub affected_cells: [usize; 3],
    pub newly_recoverable: Vec<usize>,
    /// Cell states in the same order as `affected_cells`.
    pub cells_after: [Cell; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DecodeTrace {
    pub config: Config,
    pub initial_cells: Vec<Cell>,
    pub steps: Vec<PeelStep>,
    pub final_cells: Vec<Cell>,
}

/// A summary of a set, or a signed difference after subtraction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iblt {
    config: Config,
    cells: Vec<Cell>,
}

impl Iblt {
    pub fn new(config: Config) -> Result<Self, IbltError> {
        config.validate()?;
        Ok(Self {
            config,
            cells: vec![Cell::default(); config.cells],
        })
    }

    pub fn from_set(config: Config, keys: &HashSet<u64>) -> Result<Self, IbltError> {
        let mut table = Self::new(config)?;
        for &key in keys {
            table.insert(key)?;
        }
        Ok(table)
    }

    /// Construct a table from explicitly encoded cells.
    ///
    /// This validates dimensions, not whether the cells describe a genuine set.
    /// Decoding arbitrary cells may produce an incomplete report.
    pub fn from_cells(config: Config, cells: Vec<Cell>) -> Result<Self, IbltError> {
        config.validate()?;
        if cells.len() != config.cells {
            return Err(IbltError::CellCountMismatch {
                expected: config.cells,
                actual: cells.len(),
            });
        }
        Ok(Self { config, cells })
    }

    pub fn config(&self) -> Config {
        self.config
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Insert one unique key.
    ///
    /// The caller must ensure the key has not already been inserted. This
    /// compact summary cannot reliably answer membership queries to enforce
    /// that precondition. Prefer `from_set` when building from arbitrary input.
    /// On count overflow, no cells are modified.
    pub fn insert(&mut self, key: u64) -> Result<(), IbltError> {
        let indices = locations(self.config, key);
        for &index in &indices {
            self.cells[index]
                .count
                .checked_add(1)
                .ok_or(IbltError::CountOverflow)?;
        }
        let key_checksum = checksum(self.config.seed, key);
        for index in indices {
            let cell = &mut self.cells[index];
            cell.count += 1;
            cell.key_xor ^= key;
            cell.checksum_xor ^= key_checksum;
        }
        Ok(())
    }

    /// Compute `self - other`; positive entries are only in `self`.
    pub fn subtract(&self, other: &Self) -> Result<Self, IbltError> {
        if self.config != other.config {
            return Err(IbltError::IncompatibleConfig {
                left: self.config,
                right: other.config,
            });
        }
        let cells = self
            .cells
            .iter()
            .zip(&other.cells)
            .map(|(left, right)| {
                Ok(Cell {
                    count: left
                        .count
                        .checked_sub(right.count)
                        .ok_or(IbltError::CountOverflow)?,
                    key_xor: left.key_xor ^ right.key_xor,
                    checksum_xor: left.checksum_xor ^ right.checksum_xor,
                })
            })
            .collect::<Result<Vec<_>, IbltError>>()?;
        Ok(Self {
            config: self.config,
            cells,
        })
    }

    /// Consume a table and recover as many signed keys as peeling permits.
    pub fn decode(self) -> DecodeReport {
        self.peel(false).0
    }

    /// Decode while recording cell changes for visualization.
    pub fn decode_with_trace(self) -> (DecodeReport, DecodeTrace) {
        let (report, trace) = self.peel(true);
        (report, trace.expect("trace recording was enabled"))
    }

    fn recoverable(&self, index: usize) -> Option<(u64, i32)> {
        let cell = self.cells[index];
        if cell.count != 1 && cell.count != -1 {
            return None;
        }
        let key = cell.key_xor;
        if checksum(self.config.seed, key) != cell.checksum_xor
            || !locations(self.config, key).contains(&index)
        {
            return None;
        }
        Some((key, cell.count))
    }

    fn peel(mut self, record_trace: bool) -> (DecodeReport, Option<DecodeTrace>) {
        let mut trace = record_trace.then(|| DecodeTrace {
            config: self.config,
            initial_cells: self.cells.clone(),
            steps: Vec::new(),
            final_cells: Vec::new(),
        });
        let mut queue: VecDeque<_> = (0..self.cells.len())
            .filter(|&index| self.recoverable(index).is_some())
            .collect();
        let mut only_a = Vec::new();
        let mut only_b = Vec::new();
        // Malformed externally supplied cells must not make peeling cycle.
        let mut recovered = HashSet::new();
        while let Some(source_cell) = queue.pop_front() {
            let Some((key, sign)) = self.recoverable(source_cell) else {
                continue;
            };
            if recovered.contains(&key) {
                continue;
            }
            let affected_cells = locations(self.config, key);
            // A malformed table can contain extreme counts. Leave an
            // unrepresentable update as a residual instead of wrapping.
            if affected_cells
                .iter()
                .any(|&index| self.cells[index].count.checked_sub(sign).is_none())
            {
                continue;
            }
            let before = affected_cells.map(|index| self.recoverable(index));
            let key_checksum = checksum(self.config.seed, key);
            for index in affected_cells {
                let cell = &mut self.cells[index];
                cell.count -= sign;
                cell.key_xor ^= key;
                cell.checksum_xor ^= key_checksum;
            }
            recovered.insert(key);
            if sign == 1 {
                only_a.push(key);
            } else {
                only_b.push(key);
            }
            let mut newly_recoverable = Vec::new();
            for (slot, &index) in affected_cells.iter().enumerate() {
                let after = self.recoverable(index);
                if after.is_some() {
                    queue.push_back(index);
                    if after != before[slot] {
                        newly_recoverable.push(index);
                    }
                }
            }
            if let Some(trace) = &mut trace {
                trace.steps.push(PeelStep {
                    key,
                    sign,
                    source_cell,
                    affected_cells,
                    newly_recoverable,
                    cells_after: affected_cells.map(|index| self.cells[index]),
                });
            }
        }
        only_a.sort_unstable();
        only_b.sort_unstable();
        let residual_cells = self.cells.iter().filter(|cell| !cell.is_zero()).count();
        let report = DecodeReport {
            status: if residual_cells == 0 {
                DecodeStatus::Complete
            } else {
                DecodeStatus::Incomplete
            },
            peeling_steps: only_a.len() + only_b.len(),
            only_a,
            only_b,
            residual_cells,
        };
        if let Some(trace) = &mut trace {
            trace.final_cells = self.cells;
        }
        (report, trace)
    }
}
