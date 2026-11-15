# iblt-set-reconciliation

A Rust implementation of an Invertible Bloom Lookup Table (IBLT) for reconciling
unordered sets of unique `u64` identifiers. Shared entries cancel when compatible
summaries are subtracted; a peeling decoder recovers the remaining signed keys
when the residual table is sufficiently lightly loaded.

The core library supports insertion, subtraction, deterministic hashing, and
decoding with an optional peeling trace. Positive recovered keys belong to
`A \ B`; negative recovered keys belong to `B \ A`.

```rust
use std::collections::HashSet;
use iblt_set_reconciliation::{Config, DecodeStatus, Iblt};

let config = Config::new(30, 42)?;
let a = Iblt::from_set(config, &HashSet::from([1, 2, 3]))?;
let b = Iblt::from_set(config, &HashSet::from([2, 3, 4]))?;
let report = a.subtract(&b)?.decode();
assert_eq!(report.status, DecodeStatus::Complete);
assert_eq!(report.only_a, [1]);
assert_eq!(report.only_b, [4]);
# Ok::<(), iblt_set_reconciliation::IbltError>(())
```

Each cell stores an `i32` count and two `u64` XOR fields. Hash version 1 uses
domain-separated BLAKE3 hashes, with one location in each of three equal regions.
Cell capacity must be at least three and divisible by three; both summaries must
use the same capacity and seed. Zero is a valid key. Use `from_set` for unique-key
semantics; individual `insert` calls require the caller to avoid duplicate keys.

**Incomplete recovery is explicit.** Every field of every residual cell must be
zero for decoding to report `Complete`. An empty queue or zero counts alone do
not imply success. An incomplete report retains the recovered keys and counts
the remaining nonzero cells. Checksums are probabilistic, so completion is not a
collision-free proof of equality.

Run the core checks with a current Rust toolchain supporting edition 2024:

```sh
cargo test --lib --test core
cargo clippy --lib -- -D warnings
```

Run a reconciliation of two 100,000-key sets with 50 total differences:

```sh
cargo run --release -- demo --common 99975 --a-only 25 --b-only 25 --cells 120 --seed 42 --json output/demo.json --summary output/a.iblt
```

`--data-seed` controls deterministic input generation independently of the table
hash seed. The CLI computes exact `HashSet` differences, encodes A, decodes the
received summary, subtracts B, and checks the recovered keys against the oracle.
The report includes directional keys, residual cells, peeling steps, actual
summary bytes, and separately measured construction/encoding/subtraction/decode
times. Each set can contain up to 100,000 keys.

A summary uses a 24-byte header followed by 20 bytes per cell. Header fields are
`IBLT` magic (4 bytes), format version (u16), hash/layout version (u16), cell count
(u64), and hash seed (u64); each cell is count (i32), key XOR (u64), and checksum XOR
(u64). All integers are little endian. Unknown versions, invalid capacities,
truncation, and trailing bytes are rejected. The byte comparison is one A summary
against A's raw 8-byte identifiers; it excludes transport framing and any reply.

Exit codes: `0` complete recovery matching the oracle; `1` validation or I/O error;
`2` CLI usage error; `3` incomplete recovery; `4` oracle mismatch. For example,
this deliberately stalled case exits with code 3 and retains three nonzero cells
even though their counts are zero:

```sh
cargo run --release -- demo --common 0 --a-only 1 --b-only 1 --cells 3
```

Export a small peeling cascade as a standalone SVG:

```sh
cargo run --release -- demo --common 20 --a-only 3 --b-only 2 --cells 18 --seed 42 --svg output/peeling.svg
```

Open the SVG in a browser or image viewer. Its three hash regions show counts,
full key/checksum XOR values, and signed recoverable keys. Each step highlights
the source, updated cells, and newly recoverable entries. Small traces show all
steps; large traces sample snapshots and label omitted steps, while always
showing the initial and final states. Residual mixed cells remain visibly
unresolved even with a count of zero. SVG tracing/rendering is excluded from the
reported decode time. Add `--svg output/stalled.svg` to the stalled example to
inspect failure in the same format.

Committed examples: [five-key peeling cascade](docs/peeling.svg) and
[stalled zero-count residual](docs/stalled.svg).

Background: Goodrich and Mitzenmacher,
[Invertible Bloom Lookup Tables](https://arxiv.org/abs/1101.2245).
This project focuses on sets; key-value records, networking, and automatic table
size estimation are outside its scope. No GitHub Actions workflows are used.
