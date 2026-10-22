# Transform diagnosis

Source snapshots come from `C:/code/aethos/experiments/src`; dependency versions come from its Cargo.lock. `prepare.py` regenerates snapshots and diagnostic variants. All original algorithm bodies remain intact with optional phase timing compiled out unless `phases` is enabled. `edict_3_backend` additionally exposes an untimed idle-write probe.

Build with stable (verified rustc 1.99.1, LLVM 23.1.1). The installed nightly fails compiling shipyard 0.11.5.

```powershell
cargo +stable build --release --offline --manifest-path experiments/aethos-transform-diagnosis/Cargo.toml
Copy-Item experiments/aethos-transform-diagnosis/target/release/aethos-transform-diagnosis.exe experiments/aethos-transform-diagnosis/baseline.exe
cargo +stable build --release --offline --features phases --manifest-path experiments/aethos-transform-diagnosis/Cargo.toml
Copy-Item experiments/aethos-transform-diagnosis/target/release/aethos-transform-diagnosis.exe experiments/aethos-transform-diagnosis/phases.exe
experiments/aethos-transform-diagnosis/baseline.exe --verify-only
experiments/aethos-transform-diagnosis/baseline.exe --idle-writes
experiments/aethos-transform-diagnosis/baseline.exe --samples 7 --frames 64 --count 16384
experiments/aethos-transform-diagnosis/phases.exe --backend edict_bevy --samples 7 --frames 64
```

Backend selectors: `bevy`, `edict_bevy`, `edict_alt`, `edict_narrow`, `edict_located`, `edict`, `edict_3`, `edict_3_alt`, `all` (default).

- `edict_alt`: replaces mutable Marks/Global queries with `Alt<T>`, delaying component epoch stores until mutable dereference. It still advances the world atomic epoch on each get. Current edict exposes no `as_const` view conversion.
- `edict_narrow`: cover/mark traverses a children-only hierarchy query; update retains the original optional tuple. Marker/global access remains original.
- `edict_located`: marker and hierarchy queries additionally yield `Entities`, reusing the resulting `EntityLoc` for the next query in each cover/mark/update call. Recursion and candidate lists retain `EntityId`; algorithm and mutable access semantics remain original.
- `edict_3_alt`: replaces only Global queries with `Alt<Global>`, so reads of root/parent globals do not mark them modified.

CSV timing columns use nanoseconds per frame. Total columns time complete batches with the original alternating mutation pattern, sync, and final global read. Mutation/sync columns come from separate split-timer batches, so timer overhead makes them unsuitable for exact sum comparisons. Allocation, spawning, initial sync, correctness checks, and warmup are outside timing.

The `phases` feature measures candidate collection plus cover/mark and candidate update with only boundary timers. Scheduler overhead remains in total sync. Bevy `clear_trackers` remains in total sync but outside internal phases. Baseline executables have no internal timer calls.

Every run first verifies each selected implementation against independent recursive recomputation: reversed spawning, each workload, four alternating frames with idle frames between, and four reparents. Both baseline and phase builds passed all seven implementations.

`located-comparison.exe` contains both `edict_bevy` and `edict_located` in one uninstrumented release binary; both passed the same oracle. Use it for their direct comparison. The saved `baseline.exe` and `phases.exe` retain the other seven implementations.

`shared-components.exe` controls component identity across `edict_bevy`, `edict_alt`, `edict_narrow`, and `edict_located`: all four import the same `Global`, `LocalPosition`, and `Marks` types from the baseline module. This keeps component TypeIds and component hashmap layouts identical across those variants. Their `Scratch` states remain private to each implementation. Prefer this binary for controlled comparisons; earlier saved binaries define distinct component types per variant.

Untimed probe at 16,384 entities: each of three idle syncs writes 16,384 Globals in `edict_3`, versus zero in `edict_3_alt`.
