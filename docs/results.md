# Measured results

Recorded on 2026-11-15 with Rust 1.97.1, Cargo 1.97.1, target
`x86_64-pc-windows-msvc`, and the release profile. All trials ran locally and
sequentially. Timings are machine-dependent observations rather than performance
guarantees; counts, outcomes, and bytes are reproducible with the locked
dependencies and the parameters below.

## Reliability sweep

```sh
cargo run --release -- sweep --differences 5,10,25,50,100 --cells 6,12,24,36,60,90,120,150,210,300 --trials 1000 --data-seed 7 --seed-start 0 --output output/sweep.csv --json output/sweep.json
```

This is 50,000 trials: five difference counts, ten explicit capacities, and
1,000 consecutive hash seeds (0–999) per row. For each difference count, one
fixed dataset is generated with data seed 7. Adjacent data seed values shift the
input permutation; they should not be treated as independent random datasets.
The experiment varies hash seeds, not data seeds.

Only the signed differences are constructed during this sweep. Shared keys
cancel exactly, so they cannot change decoder outcome; cancellation equivalence
is covered by integration tests. Build cost for large input sets is measured by
the separate end-to-end demos below. Decode timing excludes table construction,
oracle verification, tracing, and SVG rendering. A peeling step means one
successfully removed key, not a queue pop.

Selected observations for 50 differences:

| Cells | Summary bytes | Exact recoveries | Success rate | Mean decoded keys | Mean decode time |
|---:|---:|---:|---:|---:|---:|
| 60 | 1,224 | 85/1,000 | 8.5% | 25.42 | 36.53 µs |
| 90 | 1,824 | 947/1,000 | 94.7% | 49.84 | 81.27 µs |
| 120 | 2,424 | 973/1,000 | 97.3% | 49.94 | 87.59 µs |
| 150 | 3,024 | 988/1,000 | 98.8% | 49.98 | 90.55 µs |
| 210 | 4,224 | 997/1,000 | 99.7% | 49.99 | 97.60 µs |
| 300 | 6,024 | 997/1,000 | 99.7% | 49.99 | 104.14 µs |

The denominator includes incomplete trials; success requires completion and
exact equality with both directional `HashSet` differences. Every incomplete
output was also checked for incorrect or duplicate keys. Across all 50,000
trials, there were **zero false completions and zero invalid recovered outputs**.
This observation does not remove the possibility of checksum collisions.

The [complete CSV](results/sweep.csv) contains every capacity/count combination,
seed provenance, incomplete counts, total decoded keys and peeling steps, and
mean decode nanoseconds. Larger tables change hash locations, so individual
seeds and finite-sample rates need not improve monotonically. These are empirical
rates for the selected dataset, not universal success probabilities.

## End-to-end reconciliation

All five runs generated the full sets, built both tables, serialized and
deserialized A, subtracted B, and checked the decoded result against ordinary
`HashSet` subtraction. Hash seed 42 and data seed 7 were used throughout.

| Differences | Common | A-only | B-only | A size | B size | Cells | Summary bytes | Result |
|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 5 | 99,997 | 2 | 3 | 99,999 | 100,000 | 18 | 384 | Exact |
| 10 | 99,995 | 5 | 5 | 100,000 | 100,000 | 30 | 624 | Exact |
| 25 | 99,987 | 12 | 13 | 99,999 | 100,000 | 60 | 1,224 | Exact |
| 50 | 99,975 | 25 | 25 | 100,000 | 100,000 | 120 | 2,424 | Exact |
| 100 | 99,950 | 50 | 50 | 100,000 | 100,000 | 240 | 4,824 | Exact |

Reproduce any row using its common/A-only/B-only/cell values, for example:

```sh
cargo run --release -- demo --common 99975 --a-only 25 --b-only 25 --cells 120 --seed 42 --data-seed 7 --json output/demo-50.json
```

For that 50-difference run, table construction took 29.399 ms for A and 29.437 ms
for B; decoding took 0.103 ms. The one-way summary was 2,424 bytes, including its
24-byte header, versus 800,000 payload bytes for A's raw identifiers. Neither
number includes transport framing or a result reply. The small SVG example
actually has a larger summary than its raw set: savings are useful when the sets
are large relative to their difference, not for every input size.

Full saved reports retain recovered keys, residual counts, parameters, and all
stage timings: [5 differences](results/demo-5.json),
[10 differences](results/demo-10.json), [25 differences](results/demo-25.json),
[50 differences](results/demo-50.json), and [100 differences](results/demo-100.json).

## Explicit failure

```sh
cargo run --release -- demo --common 0 --a-only 1 --b-only 1 --cells 3 --seed 42 --data-seed 7 --svg output/stalled.svg
```

This exits with code **3**, recovers zero keys, and retains three nonzero cells.
All counts are zero, but the XOR fields are unresolved. The committed
[stalled diagram](stalled.svg) shows this state. The
[five-key cascade](peeling.svg) is a separate successful example with all steps
shown; reproduce it with common 20, A-only 3, B-only 2, capacity 18, and seed 42.
