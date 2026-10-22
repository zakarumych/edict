# Aethos transform benchmark diagnosis

The main `edict_bevy` slowdown is the cost of repeated point queries: resolving entity IDs, rediscovering component columns, rebuilding fetches, and moving the results between recursive functions. The two adapters perform essentially the same tree algorithm, but their point-query implementations perform substantially different work. The atomic epoch increment is a secondary cost. The `edict_3` adapter also has a separate change-detection feedback bug.

Production edict and Aethos sources were not edited. Diagnostic sources, controlled forks, executables, and results live under this directory. The four pre-existing edits in edict's query/system sources were preserved and included in the baseline.

## Full-workload evidence

The latest supplied depth-tagged results already show the pattern: at 16,384 entities, `edict_bevy` takes 809.040 us versus Bevy's 348.713 us for roots, and 1049.883 us versus 491.438 us for all-local changes. Idle is much faster in edict.

A fresh diagnostic run reproduces it. Values below are median microseconds per frame; compilation and fixture setup are outside timing.

| Backend | Idle | Sparse local | Roots | All local |
|---|---:|---:|---:|---:|
| Bevy | 10.602 | 27.354 | 344.120 | 489.790 |
| edict_bevy | 0.271 | 35.341 | 824.044 | 1045.184 |
| edict | 8.818 | 53.377 | 863.388 | 1103.158 |
| edict_3 | 720.898 | 744.335 | 724.439 | 979.682 |
| edict_3 with lazy Global tracking | 0.141 | 37.149 | 726.131 | 959.257 |

Source: [complete pinned run](aethos-transform-diagnosis/results-baseline-pinned.csv). These are a separate run and harness from the supplied Criterion estimates; do not mix their absolute times or uncertainty measures.

Boundary instrumentation localizes the gap to the actual traversal. These are separate instrumented runs, in microseconds:

| Workload/backend | Input mutation | Candidate collection + marking | Propagation |
|---|---:|---:|---:|
| Roots / Bevy | 1.399 | 153.484 | 191.079 |
| Roots / edict_bevy | 1.808 | 370.426 | 422.592 |
| All local / Bevy | 106.810 | 177.640 | 197.509 |
| All local / edict_bevy | 145.547 | 456.093 | 436.849 |

Sources: [Bevy phases](aethos-transform-diagnosis/results-phases-bevy-pinned.csv), [edict phases](aethos-transform-diagnosis/results-phases-edict-pinned.csv). The marking phase contains no transform composition, yet is over twice as expensive for roots. Mutation explains only a small portion of the all-local gap. Scheduler and lock-acquisition costs cannot explain hundreds of microseconds when the entire edict_bevy idle frame takes about 0.27 us. Phase medians come from separate samples and are not an exact additive accounting of the uninstrumented total.

## What each edict point query does

The relevant implementation chain is:

1. [ViewValue::try_get/try_get_mut](C:/code/edict/src/view/index.rs:39) resolves the entity through `Entity::lookup`.
2. For an `EntityId`, [EntitySet::get_location](C:/code/edict/src/entity/set.rs:196) searches `HashMap<u64, Location>`.
3. [get_at](C:/code/edict/src/view/mod.rs:471) checks the archetype, runs query/filter matching, obtains an epoch, constructs fetches, and executes the chunk/item protocol.
4. [Read::visit_archetype and Read::fetch](C:/code/edict/src/query/read.rs:115) separately test component presence and retrieve its column. Both use the archetype's TypeId hash table. [OptionQuery::fetch](C:/code/edict/src/query/option.rs:157) performs the same inner presence/fetch sequence for each present optional component.
5. Mutable gets also increment the world epoch and write archetype, chunk, and entity epochs through [FetchWrite](C:/code/edict/src/query/write.rs:39).

The duplicate component probes survive compilation. The emitted scalar optional-tuple `get_at` contains distinct hash-table probe loops for presence and column retrieval. This is not merely duplicate-looking Rust that LLVM eliminates.

Bevy 0.20.0's local source takes a different route: `Entities::get_spawned` indexes its entity metadata array and checks generation; `Query::get_inner` checks a cached matched-archetype bitset; fetch state contains numeric component IDs used to index table columns. Bevy also initializes a fetch for a point query, but avoids edict's repeated TypeId hash lookup machinery. Relevant local files are `bevy_ecs-0.20.0/src/entity/mod.rs:844`, `src/system/query.rs:1873`, and `src/query/fetch.rs:2459` under the Cargo registry.

The root-update workload amplifies these differences. Counting calls in the current adapter gives 65,729 point queries during synchronization: 16,577 marker gets, 32,768 hierarchy gets, and 16,384 global gets. This excludes 193 input writes and the final black-box output read. About 33,000 of those view gets are mutable. The forest has only four levels; deep recursion is not required to expose the cost.

## Isolated access measurements

Matching scalar components, 16,384 entities, four archetypes, locked edict views and prepared Bevy queries. Median nanoseconds per entity, interleaved entity order:

| Operation | edict | Bevy |
|---|---:|---:|
| Entity location lookup alone | 1.792 | 0.773 |
| Scalar read by entity ID | 6.529 | 1.469 |
| Three optional component reads by ID | 13.390 | 3.811 |
| Mutable scalar access by ID | 14.805 | 2.469 |
| Scalar read through iteration | 0.102 | 0.102 |
| Scalar write through iteration | 0.324 | 0.614 |

The iteration controls show that edict's storage can be traversed efficiently when query setup is amortized. These measurements support a point-access problem, not a general claim that all edict iteration or storage is slower.

Further edict-only isolation:

| Read path | Scalar | Three optional components |
|---|---:|---:|
| Public get by ID | 6.529 ns | 13.390 ns |
| Public get with pre-resolved EntityLoc | 2.468 ns | 6.548 ns |
| Fetch each scalar from its known matching archetype | 1.363 ns | — |
| Pre-resolved location and cached per-archetype fetch | 0.279 ns | 0.615 ns |

The last two rows are diagnostic lower bounds: matching/location validation and preparation happen outside timing, immutable world borrows keep locations valid, and locked views guard access. They are not drop-in public API replacements. Their purpose is to remove particular layers while preserving the component reads. Shuffled access gives the same qualitative conclusion. Individual microbenchmark deltas cannot be multiplied and added to claim an exact percentage of the full transform workload: dependencies, layout, inlining, and cache behavior interact.

Source and all 40 cases: [access harness](aethos-access/src/main.rs), [pinned results including min/max](aethos-access/results-stable-pinned.txt), [methodology](aethos-access/README.md).

## Atomic epoch control

A private fork replaces only `EpochCounter::next`'s atomic `fetch_add(Relaxed)` with relaxed load/store. It preserves sequential epoch values but is deliberately unsafe for concurrent use. It is an attribution experiment, not a proposed production implementation.

Both executables are compiled at the same dependency and harness paths, with identical component type identities. Runs use baseline/ablated/ablated/baseline order. Averaging the two run medians:

| Workload | Normal epoch | Load/store diagnostic | Time reduction |
|---|---:|---:|---:|
| Roots | 772.651 us | 764.402 us | 1.1% |
| All local | 1024.895 us | 957.317 us | 6.6% |

The root difference is small relative to general run variability. Even removing the atomic read-modify-write leaves almost the whole root-update disadvantage. Atomic traffic matters more for all-local input writes and extra marker accesses, but does not explain the 2x result.

Sources: [baseline pair](epoch-ablation/results-transform-baseline-pinned-complete.csv), [ablated pair](epoch-ablation/results-transform-ablated-pinned-complete.csv), [fork and reproduction instructions](epoch-ablation/README.md). Interrupted `*-pinned.csv` files without `-complete` are excluded.

## Diagnostic adapter changes

The `shared-components.exe` comparisons use the same Global, LocalPosition, Marks, Level and ChildOf component types for baseline, lazy tracking, narrower hierarchy queries, and located queries. This matters because separate component definitions change TypeIds and the hash-table layout even within one executable.

- A children-only query during marking helps, but does not close the gap. The two root-update runs take 722 and 673 us, versus baseline runs at 774 and 783 us. All-local runs take 965 and 921 us, versus 1017 and 1032 us. Avoid fetching unused parent/local fields in this phase, while treating the exact improvement as layout/run dependent.
- Using `Alt<Marks>` and `Alt<Global>` to avoid epoch stores on read-only use does not fix edict_bevy: roots take 815–822 us and all-local 1051–1052 us. It still advances the world epoch for mutable query access and still rebuilds the fetch.
- Adding `Entities` to queries and reusing the returned EntityLoc is **not** a demonstrated optimization. With shared component identities, roots take 854–861 us and all-local 1105–1118 us. The saved map lookup comes with a larger result, an entity-ID-column read, and different call/inlining/stack behavior. The scalar EntityLoc microbenchmark does not imply that enlarging recursive query tuples will improve the whole workload.

Raw files are `aethos-transform-diagnosis/results-*-shared-pinned.csv`. Keep the unsuccessful variants as evidence against recommending unmeasured changes.

[Assembly inspection of the shared-component build](aethos-transform-diagnosis/assembly-findings.md) confirms that the located hierarchy getter calls an out-of-line `get_at` and copies its 48-byte result, while the baseline hierarchy getter contains `get_at` inline. The recursive update's stack allocation grows from 152 to 184 bytes. These are concrete offsetting costs; their individual timing contributions have not been separated.

## Separate edict_3 feedback bug

In [edict_3::update](C:/code/aethos/experiments/src/edict_3_backend.rs:93), `globals.get_mut(parent).copied()` and `globals.get_mut(e).copied()` obtain mutable references merely to read parent/root values. Ordinary `&mut Global` fetches mark them changed immediately.

The `traverse` system's `Modified<Global>` cutoff advances when traverse finishes. The later update system touches root globals after that cutoff. On the next frame, traverse therefore sees every root as changed again and repeats the forest traversal.

An untimed probe counts modified Globals across three consecutive idle syncs:

```text
edict_3:       [16384, 16384, 16384]
edict_3_alt:   [0, 0, 0]
```

Changing only Global access to `Alt<Global>` in the diagnostic adapter prevents reads from changing component epochs. Idle time falls from 720.898 us to 0.141 us. Root-update cost remains about 725 us, as expected: actual propagation still uses the same expensive point-access path. This feedback does not explain edict_bevy: its collection and propagation occur in one system, whose modification-filter cutoff advances afterward.

## Validation and reproduction

Compiler: stable Rust 1.99.1, LLVM 23.1.1, x86_64-pc-windows-msvc. Dependencies match the experiment lockfile, including Bevy ECS 0.20.0. Reported pinned timings use processor-affinity mask 4 (one logical processor). Our compilation and benchmark jobs were serialized. Preliminary unpinned results are not used here.

The access harness passes all 40 checksum checks. All eight transform adapters pass the independent transform oracle with reversed spawn order, all four workloads, alternating mutations, intervening idle frames, and reparents. Timed forests have 16,384 entities and are initially checked outside timing. Both atomic-control binaries pass their snapshot's six-backend oracle. These checks validate diagnostic behavior; no production performance fix or concurrency redesign was implemented or validated.

```powershell
# From C:\code\edict; build before timing.
cargo +stable build --release --offline --manifest-path experiments/aethos-access/Cargo.toml
cargo +stable build --release --offline --manifest-path experiments/aethos-transform-diagnosis/Cargo.toml

# The shell's child processes inherit this affinity.
[System.Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity = [IntPtr]4
experiments/aethos-access/target/release/aethos-access-diagnostics.exe 150
experiments/aethos-transform-diagnosis/target/release/aethos-transform-diagnosis.exe --samples 7 --frames 96
experiments/aethos-transform-diagnosis/target/release/aethos-transform-diagnosis.exe --idle-writes
```

For boundary timers, build the transform harness with `--features phases`; save a separate executable before switching features. The atomic-fork README documents its controlled pair build. To emit the access assembly, use `cargo +stable rustc --release --offline --manifest-path experiments/aethos-access/Cargo.toml -- --emit=asm`.

The main optimization target is a cheaper prepared point-query path: avoid repeated component-type discovery, preserve useful location information without enlarging every hot return value, and check the resulting machine code. Any production cache must respect world changes, borrow lifetimes, query filters and epoch semantics. The measured next step is not simply removing atomics or copying Bevy's tree algorithm again.
