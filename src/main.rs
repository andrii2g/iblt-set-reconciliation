use clap::{Args, Parser, Subcommand};
use iblt_set_reconciliation::{
    Config, DecodeReport, DecodeStatus, Iblt,
    dataset::{generate_sets, verify, verify_partial},
    encoding,
    experiment::{SweepOptions, run_sweep, sweep_csv},
    svg,
};
use serde::Serialize;
use std::{fs, path::Path, path::PathBuf, process::ExitCode, time::Instant};

#[derive(Parser)]
#[command(
    name = "iblt-reconcile",
    version,
    about = "Reconcile unordered u64 sets with invertible Bloom lookup tables"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate two sets, reconcile their summaries, and check against HashSet.
    Demo(DemoArgs),
    /// Measure recovery across explicit capacities and reproducible hash seeds.
    Sweep(SweepArgs),
}

#[derive(Args)]
struct SweepArgs {
    /// Total symmetric differences; odd values use floor(d/2) A-only keys.
    #[arg(long, value_delimiter = ',', default_value = "5,10,25,50,100")]
    differences: Vec<usize>,
    /// Explicit capacities, each at least 3 and divisible by 3.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "6,12,24,36,60,90,120,150,210,300"
    )]
    cells: Vec<usize>,
    /// Consecutive hash seeds per capacity and difference count.
    #[arg(long, default_value_t = 1000)]
    trials: usize,
    /// Input-key sample, held fixed while varying hash seeds/capacities.
    #[arg(long, default_value_t = 7)]
    data_seed: u64,
    #[arg(long, default_value_t = 0)]
    seed_start: u64,
    /// Save all aggregate measurements, including seed provenance, as CSV.
    #[arg(long, default_value = "output/sweep.csv", value_name = "FILE")]
    output: PathBuf,
    /// Also save aggregate measurements as JSON.
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
}

#[derive(Args)]
struct DemoArgs {
    /// Keys shared by both sets (each resulting set must have <=100000 keys).
    #[arg(long, default_value_t = 99975)]
    common: usize,
    /// Keys present only in A.
    #[arg(long, default_value_t = 25)]
    a_only: usize,
    /// Keys present only in B.
    #[arg(long, default_value_t = 25)]
    b_only: usize,
    /// Explicit total cell capacity; at least 3 and divisible by 3.
    #[arg(long, default_value_t = 120)]
    cells: usize,
    /// Hash seed shared by both summaries.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Independent seed for reproducible input sets.
    #[arg(long, default_value_t = 7)]
    data_seed: u64,
    /// Save the reconciliation report as JSON.
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
    /// Save the transmitted A summary in the versioned binary format.
    #[arg(long, value_name = "FILE")]
    summary: Option<PathBuf>,
    /// Save a standalone cell diagram of the actual peeling trace.
    #[arg(long, value_name = "FILE")]
    svg: Option<PathBuf>,
}

#[derive(Serialize)]
struct DemoReport {
    schema_version: u8,
    config: Config,
    data_seed: u64,
    common: usize,
    set_a_keys: usize,
    set_b_keys: usize,
    expected_only_a: usize,
    expected_only_b: usize,
    summary_bytes: usize,
    raw_a_bytes: usize,
    decoded_keys: usize,
    oracle_match: bool,
    recovered_keys_valid: bool,
    decode: DecodeReport,
    timings: Timings,
}

#[derive(Serialize)]
struct Timings {
    build_a_ns: u128,
    build_b_ns: u128,
    encode_ns: u128,
    deserialize_ns: u128,
    subtract_ns: u128,
    decode_ns: u128,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Demo(args) => demo(args),
        Command::Sweep(args) => sweep(args),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn sweep(args: SweepArgs) -> Result<u8, String> {
    let options = SweepOptions {
        differences: args.differences,
        capacities: args.cells,
        trials: args.trials,
        data_seed: args.data_seed,
        seed_start: args.seed_start,
    };
    println!(
        "Reliability sweep: {} trials per row, hash seed start={}, data seed={}",
        options.trials, options.seed_start, options.data_seed
    );
    println!("Timing covers decoding signed differences; shared keys and tracing are excluded.");
    let rows = run_sweep(&options)?;
    write_output(&args.output, sweep_csv(&rows).as_bytes())?;
    if let Some(path) = &args.json {
        let json = serde_json::to_vec_pretty(&rows).map_err(|err| err.to_string())?;
        write_output(path, &json)?;
    }
    println!(
        " diff  cells   bytes  exact/trials  success   incomplete  mean keys  mean steps  decode us"
    );
    for row in &rows {
        println!(
            "{:5} {:6} {:7} {:5}/{:<6} {:7.2}% {:11} {:10.2} {:11.2} {:10.2}",
            row.differences,
            row.cells,
            row.summary_bytes,
            row.successes,
            row.trials,
            row.success_rate * 100.0,
            row.incomplete,
            row.decoded_keys_total as f64 / row.trials as f64,
            row.peeling_steps_total as f64 / row.trials as f64,
            row.mean_decode_ns / 1000.0
        );
    }
    let false_completions: usize = rows.iter().map(|row| row.false_completions).sum();
    let partial_mismatches: usize = rows.iter().map(|row| row.partial_mismatches).sum();
    println!(
        "Oracle mismatches: {false_completions} false completions; {partial_mismatches} invalid recovered outputs"
    );
    println!("CSV measurements: {}", args.output.display());
    if let Some(path) = &args.json {
        println!("JSON measurements: {}", path.display());
    }
    Ok(if false_completions == 0 && partial_mismatches == 0 {
        0
    } else {
        4
    })
}

fn demo(args: DemoArgs) -> Result<u8, String> {
    let config = Config::new(args.cells, args.seed).map_err(|err| err.to_string())?;
    let sets = generate_sets(args.common, args.a_only, args.b_only, args.data_seed)?;
    let start = Instant::now();
    let a = Iblt::from_set(config, &sets.a).map_err(|err| err.to_string())?;
    let build_a_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let b = Iblt::from_set(config, &sets.b).map_err(|err| err.to_string())?;
    let build_b_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let wire = encoding::encode(&a);
    let encode_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let received_a = encoding::decode(&wire).map_err(|err| err.to_string())?;
    let deserialize_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let delta = received_a.subtract(&b).map_err(|err| err.to_string())?;
    let subtract_ns = start.elapsed().as_nanos();
    // Keep visualization work outside the decoder timing used in the report.
    let trace_input = args.svg.as_ref().map(|_| delta.clone());
    let start = Instant::now();
    let decoded = delta.decode();
    let decode_ns = start.elapsed().as_nanos();
    let diagram = trace_input.map(|table| {
        let (traced, trace) = table.decode_with_trace();
        debug_assert_eq!(traced, decoded);
        svg::render_svg(&trace)
    });

    let oracle_match = verify(&decoded, &sets);
    let recovered_keys_valid = verify_partial(&decoded, &sets);
    let code =
        if !recovered_keys_valid || (decoded.status == DecodeStatus::Complete && !oracle_match) {
            4
        } else if decoded.status == DecodeStatus::Incomplete {
            3
        } else {
            0
        };
    let report = DemoReport {
        schema_version: 1,
        config,
        data_seed: args.data_seed,
        common: args.common,
        set_a_keys: sets.a.len(),
        set_b_keys: sets.b.len(),
        expected_only_a: sets.only_a.len(),
        expected_only_b: sets.only_b.len(),
        summary_bytes: wire.len(),
        raw_a_bytes: sets.a.len() * 8,
        decoded_keys: decoded.only_a.len() + decoded.only_b.len(),
        oracle_match,
        recovered_keys_valid,
        decode: decoded,
        timings: Timings {
            build_a_ns,
            build_b_ns,
            encode_ns,
            deserialize_ns,
            subtract_ns,
            decode_ns,
        },
    };
    if let Some(path) = &args.summary {
        write_output(path, &wire)?;
    }
    if let Some(path) = &args.json {
        let json = serde_json::to_vec_pretty(&report).map_err(|err| err.to_string())?;
        write_output(path, &json)?;
    }
    if let (Some(path), Some(diagram)) = (&args.svg, &diagram) {
        write_output(path, diagram.as_bytes())?;
    }
    print_demo(&report);
    if let Some(path) = &args.summary {
        println!("Binary summary: {}", path.display());
    }
    if let Some(path) = &args.json {
        println!("JSON report: {}", path.display());
    }
    if let Some(path) = &args.svg {
        println!("SVG peeling diagram: {}", path.display());
    }
    Ok(code)
}

fn print_demo(report: &DemoReport) {
    let status = match report.decode.status {
        DecodeStatus::Complete => "COMPLETE",
        DecodeStatus::Incomplete => "INCOMPLETE",
    };
    println!("Reconciliation: {status}");
    println!(
        "Sets: A={} B={} common={}",
        report.set_a_keys, report.set_b_keys, report.common
    );
    println!(
        "Table: {} cells, 3 locations/key, hash seed={}, data seed={}",
        report.config.cells, report.config.seed, report.data_seed
    );
    println!(
        "One-way summary: {} bytes ({} header + {} cell payload)",
        report.summary_bytes,
        encoding::HEADER_BYTES,
        report.summary_bytes - encoding::HEADER_BYTES
    );
    println!(
        "Raw A identifiers: {} bytes (excludes framing)",
        report.raw_a_bytes
    );
    println!(
        "Decoded: {} keys; A-only={}/{} B-only={}/{}",
        report.decoded_keys,
        report.decode.only_a.len(),
        report.expected_only_a,
        report.decode.only_b.len(),
        report.expected_only_b
    );
    println!(
        "Peeling steps: {}; residual nonzero cells: {}",
        report.decode.peeling_steps, report.decode.residual_cells
    );
    println!(
        "HashSet oracle: {}; recovered keys valid: {}",
        if report.oracle_match {
            "EXACT MATCH"
        } else {
            "NOT AN EXACT MATCH"
        },
        report.recovered_keys_valid
    );
    println!("A-only keys: {:?}", report.decode.only_a);
    println!("B-only keys: {:?}", report.decode.only_b);
    println!(
        "Timing (ms): build A={:.3}, build B={:.3}, encode={:.3}, deserialize={:.3}, subtract={:.3}, decode={:.3}",
        report.timings.build_a_ns as f64 / 1e6,
        report.timings.build_b_ns as f64 / 1e6,
        report.timings.encode_ns as f64 / 1e6,
        report.timings.deserialize_ns as f64 / 1e6,
        report.timings.subtract_ns as f64 / 1e6,
        report.timings.decode_ns as f64 / 1e6
    );
    if report.decode.status == DecodeStatus::Incomplete {
        println!("Recovery stalled. The listed keys are partial; the difference is unresolved.");
    }
}

fn write_output(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|err| format!("{}: {err}", path.display()))
}
