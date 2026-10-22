# Diagnostic epoch RMW ablation

**SINGLE-THREAD DIAGNOSTIC ONLY. This fork is not safe for concurrent ECS use.**

`edict-single-thread` is a source copy of the current working-tree edict,
including pre-existing uncommitted changes. Its only source difference is
`EpochCounter::next`: the atomic read-modify-write is replaced by a relaxed
load followed by a relaxed store. Sequential epoch values are preserved;
concurrent callers can lose updates. This isolates the cost of the atomic
read-modify-write instruction, not the correctness of an alternative design.

No production edict source is modified. Benchmark harness copies under this
directory must run on one thread. Compare each against its original harness
with identical inputs, compiler profile, and process affinity. Do not run
compilation or other benchmarks concurrently with timing.

Recreate the fork and harnesses from the current working-tree inputs:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File experiments/epoch-ablation/prepare-fork.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File experiments/epoch-ablation/copy-harness.ps1 -Harness aethos-access
powershell -NoProfile -ExecutionPolicy Bypass -File experiments/epoch-ablation/copy-harness.ps1 -Harness aethos-transform-diagnosis
```

Preparation copies only `src`, `proc`, `proc-lib`, `examples`, and `Cargo.toml`,
including uncommitted edits; `.git` and `target` are excluded. An existing fork
is preserved in an ignored backup directory within this experiment. Source
hash checks establish that only `src/epoch.rs` differs. Existing executables
remain untouched until the build command runs.

The comparison
toolchain is installed `stable` (Rust 1.99.1), matching the original results.
The copied harness README describes its executable arguments. Readme command
paths refer to the original harness; insert `epoch-ablation/` after
`experiments/` when invoking this copy.

For the actual comparison, build both variants at the same dependency and
harness paths. This controls crate identity and associated TypeId differences:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File experiments/epoch-ablation/build-controlled-pair.ps1
experiments/epoch-ablation/binaries/access-baseline.exe 100
experiments/epoch-ablation/binaries/access-ablated.exe 100
experiments/epoch-ablation/binaries/transform-baseline.exe --backend edict_bevy --samples 7 --frames 64 --count 16384
experiments/epoch-ablation/binaries/transform-ablated.exe --backend edict_bevy --samples 7 --frames 64 --count 16384
```

The script saves four executables under `binaries/` and restores diagnostic
source after building. The target directory's unsuffixed executables contain
the last-built baseline; use the explicitly named executables in `binaries/`.

SHA-256 for the paired binaries built on 2026-10-22 (the transform snapshot
contains six backend variants; newer sibling source may add diagnostics):

```text
access-baseline.exe    A26A68E98F607A6611E85BB9156B524CCF359DFA6A84FF5FDC725169A1C4353C
access-ablated.exe     CE22D73A78A22B11797F5D4FE1361F72E8705E1B8528B6F997693D2FC3918EC3
transform-baseline.exe 5939A8C5E0C1441F235132AF64A9EC279327D20D07E40268F502D0C4F59DAACD
transform-ablated.exe  CE536A35DB94EE52A1E901B991E49A48F2B78946495C896C6EFEE31AE63BD7A1
```
