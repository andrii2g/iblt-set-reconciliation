# iblt-set-reconciliation

A Rust implementation of an Invertible Bloom Lookup Table (IBLT) for reconciling
unordered sets of unique `u64` identifiers. Shared entries cancel when compatible
summaries are subtracted; a peeling decoder recovers the remaining signed keys
when the residual table is sufficiently lightly loaded.

The core library supports insertion, subtraction, deterministic hashing, and
decoding with an optional peeling trace. Positive recovered keys belong to
`A \\ B`; negative recovered keys belong to `B \\ A`.

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

Background: Goodrich and Mitzenmacher,
[Invertible Bloom Lookup Tables](https://arxiv.org/abs/1101.2245).
This project focuses on sets; key-value records, networking, and automatic table
size estimation are outside its scope. No GitHub Actions workflows are used.
