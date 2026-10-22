# ECS access diagnostics

Standalone diagnostic executable. Uses working-tree edict and pinned
`bevy_ecs = 0.20.0`; does not modify either production implementation.

```powershell
cargo +stable build --release --offline --manifest-path experiments/aethos-access/Cargo.toml
./experiments/aethos-access/target/release/aethos-access-diagnostics.exe 0
./experiments/aethos-access/target/release/aethos-access-diagnostics.exe 100
./experiments/aethos-access/target/release/aethos-access-diagnostics.exe 200 tuple
```

First argument: milliseconds per sample (default 100, zero verifies checksums
without timing). Second argument: substring filter (default all cases).
Five samples report median and min/max nanoseconds per entity. Each batch visits
16,384 entities. Run without concurrent compiles or benchmarks.

Use installed stable Rust 1.99.1 to match the original experiment toolchain.

Matching components populate four distinct depth-tag archetypes. Parent is absent
at depth 0, Children absent at depth 3. These are plain scalar components rather
than native relationships: tuple cases isolate optional component fetch
overhead, not relationship iterator semantics. Entities spawn in interleaved
depth order. Second fixture applies an identical deterministic permutation
to both engines' entity lists. Iteration ignores these lists and follows storage
order, measuring amortization available to linear traversal.

All fixture creation, query preparation, locking, and EntityLoc collection happen
outside timing. Edict views stay locked; Bevy QueryState produces a Query once
outside timing. This matches repeatedly accessing entities from prepared system
queries. Direct World writes intentionally measure their full per-call API cost.
Checksums are asserted before and after each timed case. Inputs and batch outputs
pass through black_box; write cases use opaque values. No fixture allocation or
entity-order randomization occurs inside timed loops.

Comparisons:

- `lookup`: entity location resolution only; returned location escapes through
  black_box. Edict EntityLoc and Bevy EntityLocation have different layouts.
- `read_id` vs `read_loc`: removes edict entity hash-map lookup while retaining
  its ordinary component query machinery. Location slice elements are larger
  than IDs, so this is not an exact subtraction model.
- `tuple_id` vs `tuple_loc`: three optional components, matching hierarchy-style
  query shape with equal component values and presence across engines.
- `write_id` vs `write_loc`: mutable access including native change tracking.
- `read_iter` / `write_iter`: amortized query setup and storage traversal.
- `world_write`: public World mutation API, including per-call bookkeeping.
- `read_loc_fetch_each`: uses validated known locations and archetypes, creates a
  read Fetch per entity. Removes per-get component presence checks but retains
  the component lookup performed by Query::fetch.
- `read_loc_cached_fetch` / `tuple_loc_cached_fetch`: reuse read-only fetches
  prepared per archetype before timing, removing repeated component lookup and
  fetch setup. Original locked views guard access throughout. This uses the
  unsafe public query machinery solely as diagnostic isolation, not a proposed
  application API. The fetch visit protocol mirrors edict's private chunk size
  of 256; update that diagnostic constant if the production constant changes.

These are controlled scalar microbenchmarks, not transform benchmarks. Absolute
times do not establish fraction of a full transform workload explained by
each mechanism. Compare alongside original transform scenarios and isolated
implementation ablations.
