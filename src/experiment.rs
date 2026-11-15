//! Reliability experiments over the signed symmetric difference.
//!
//! Shared keys cancel before decoding, so repeating their insertion would
//! change build time but not the residual table or outcome being measured.

use std::fmt::Write;
use std::time::Instant;

use serde::Serialize;

use crate::dataset::{MAX_SET_SIZE, generate_sets, verify, verify_partial};
use crate::encoding::{CELL_BYTES, HEADER_BYTES};
use crate::{Config, DecodeStatus, Iblt};

/// Explicit experiment choices; capacities are never estimated or retried.
#[derive(Debug, Clone)]
pub struct SweepOptions {
    pub differences: Vec<usize>,
    pub capacities: Vec<usize>,
    pub trials: usize,
    pub data_seed: u64,
    pub seed_start: u64,
}

/// Aggregated results for one difference count and one table capacity.
#[derive(Debug, Clone, Serialize)]
pub struct SweepRow {
    pub differences: usize,
    pub cells: usize,
    pub trials: usize,
    pub data_seed: u64,
    pub seed_start: u64,
    pub successes: usize,
    pub incomplete: usize,
    /// Decoder claimed completion, but the exact oracle disagreed.
    pub false_completions: usize,
    /// Trials with an incorrect directional key or duplicate output. This can
    /// overlap `false_completions` and includes incomplete decoder reports.
    pub partial_mismatches: usize,
    pub success_rate: f64,
    /// One serialized summary, including the 24-byte configuration header.
    pub summary_bytes: usize,
    pub decoded_keys_total: usize,
    pub peeling_steps_total: usize,
    /// Decode only: excludes set/table construction and exact verification.
    pub mean_decode_ns: f64,
}

/// Sweep fixed generated differences across capacities and consecutive seeds.
///
/// The same key sample and trial seed are reused across capacities. A separate
/// `data_seed` changes the key sample without conflating it with the table seed.
pub fn run_sweep(options: &SweepOptions) -> Result<Vec<SweepRow>, String> {
    if options.differences.is_empty() || options.capacities.is_empty() {
        return Err("at least one difference count and capacity are required".into());
    }
    if options.trials == 0 {
        return Err("trials must be greater than zero".into());
    }
    let last_trial = u64::try_from(options.trials - 1)
        .map_err(|_| "trial count does not fit in a u64 seed range")?;
    options
        .seed_start
        .checked_add(last_trial)
        .ok_or("trial seeds would overflow u64")?;

    // Validate every choice before starting potentially expensive work.
    for &cells in &options.capacities {
        Config::new(cells, options.seed_start).map_err(|error| error.to_string())?;
        summary_bytes(cells)?;
    }
    for &differences in &options.differences {
        if differences > 2 * MAX_SET_SIZE {
            return Err(format!(
                "differences must not exceed {} for the supported set sizes",
                2 * MAX_SET_SIZE
            ));
        }
        differences
            .checked_mul(options.trials)
            .ok_or("aggregate decoded key count would overflow usize")?;
    }
    let row_count = options
        .differences
        .len()
        .checked_mul(options.capacities.len())
        .ok_or("sweep row count overflows usize")?;
    let mut rows = Vec::new();
    rows.try_reserve(row_count)
        .map_err(|error| format!("cannot allocate sweep results: {error}"))?;

    for &differences in &options.differences {
        let a_only = differences / 2;
        let sets = generate_sets(0, a_only, differences - a_only, options.data_seed)?;
        for &cells in &options.capacities {
            let mut row = SweepRow {
                differences,
                cells,
                trials: options.trials,
                data_seed: options.data_seed,
                seed_start: options.seed_start,
                successes: 0,
                incomplete: 0,
                false_completions: 0,
                partial_mismatches: 0,
                success_rate: 0.0,
                summary_bytes: summary_bytes(cells)?,
                decoded_keys_total: 0,
                peeling_steps_total: 0,
                mean_decode_ns: 0.0,
            };
            let mut decode_ns = 0u128;
            for trial in 0..options.trials {
                // The upper bound was checked before entering the sweep.
                let seed = options.seed_start + trial as u64;
                let config = Config::new(cells, seed).map_err(|error| error.to_string())?;
                let table_a = Iblt::from_set(config, &sets.a).map_err(|error| error.to_string())?;
                let table_b = Iblt::from_set(config, &sets.b).map_err(|error| error.to_string())?;
                let difference = table_a
                    .subtract(&table_b)
                    .map_err(|error| error.to_string())?;

                let started = Instant::now();
                let report = difference.decode();
                decode_ns += started.elapsed().as_nanos();

                if verify(&report, &sets) {
                    row.successes += 1;
                } else if report.status == DecodeStatus::Complete {
                    row.false_completions += 1;
                } else {
                    row.incomplete += 1;
                }
                if !verify_partial(&report, &sets) {
                    row.partial_mismatches += 1;
                }
                row.decoded_keys_total = row
                    .decoded_keys_total
                    .checked_add(report.only_a.len())
                    .and_then(|count| count.checked_add(report.only_b.len()))
                    .ok_or("aggregate decoded key count overflows usize")?;
                row.peeling_steps_total = row
                    .peeling_steps_total
                    .checked_add(report.peeling_steps)
                    .ok_or("aggregate peeling step count overflows usize")?;
            }
            row.success_rate = row.successes as f64 / row.trials as f64;
            row.mean_decode_ns = decode_ns as f64 / row.trials as f64;
            rows.push(row);
        }
    }
    Ok(rows)
}

fn summary_bytes(cells: usize) -> Result<usize, String> {
    cells
        .checked_mul(CELL_BYTES)
        .and_then(|bytes| bytes.checked_add(HEADER_BYTES))
        .ok_or_else(|| "encoded summary length overflows usize".into())
}

/// Render numeric experiment rows as CSV, with one header and a final newline.
pub fn sweep_csv(rows: &[SweepRow]) -> String {
    let mut output = String::from(
        "differences,cells,trials,data_seed,seed_start,successes,incomplete,false_completions,partial_mismatches,success_rate,summary_bytes,decoded_keys_total,peeling_steps_total,mean_decode_ns\n",
    );
    for row in rows {
        writeln!(
            output,
            "{},{},{},{},{},{},{},{},{},{:.6},{},{},{},{:.2}",
            row.differences,
            row.cells,
            row.trials,
            row.data_seed,
            row.seed_start,
            row.successes,
            row.incomplete,
            row.false_completions,
            row.partial_mismatches,
            row.success_rate,
            row.summary_bytes,
            row.decoded_keys_total,
            row.peeling_steps_total,
            row.mean_decode_ns,
        )
        .expect("writing to a String cannot fail");
    }
    output
}
