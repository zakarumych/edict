# ECS integration tests

These tests exercise the public API without additional dependencies.

| Target | Contracts |
| --- | --- |
| `world_lifecycle` | Entity identity and reservation, lazy batch spawning, component and bundle mutations, external registration, dynamic builders, ownership hooks, alignment, and deterministic reference-model storage checks |
| `queries` | Presence and boolean truth tables, optional and borrowed components, alias rejection and recovery, change epochs, archetype transitions, and batched access |
| `batch_iteration` | Filtered read/write batches, chunk boundaries, partial iteration followed by folding, exact lengths, change epochs, and zero-size rejection without borrow leaks |
| `relations` | Directed, symmetric, exclusive and owned edges; reverse lookup; replacement and lifecycle hooks; reparenting and cascading despawn |
| `execution` | Deferred command order, cross-thread senders, resource borrowing, scheduler state and dependencies, and flow wakeup and cancellation |
| `dump` | Component presence masks, filtered and incremental dumps, load/update preservation, marking, and error propagation |
| `interactions_world` | Reserved dynamic spawns, reparenting, hook-driven archives, cascades, threaded command ingress, and selective mutation through mixed archetypes |
| `interactions_runtime` | Multi-tick transactions across scheduler runners, deferred visibility, automatic change cutoffs, and flows coordinated with scheduled work and cascade cancellation |
| `interactions_snapshot` | Checkpoint/delta replication, reconstructed relations, hook cleanup, explicit removal propagation, and resumed simulation through memory, nanoserde, and alkahest backends |

Run the complete workspace, including doctests, with all optional features:

```text
cargo test --workspace --all-features --no-fail-fast
```

Run the minimal feature configuration:

```text
cargo test --workspace --no-default-features --no-fail-fast
```

Run a target or an individual regression:

```text
cargo test --test queries --all-features
cargo test --test queries batched_mutation_updates_values_and_change_detection -- --exact
```

Feature gates select scheduler implementations and flow tests. Flow execution
requires both `flow` and `std`; the tests do not supply the platform TLS hooks
required by `flow` without `std`. Integration test harnesses use the host standard
library even when edict is built without default features.

Builder ownership tests run themselves in child processes. A library heap fault
therefore produces a failed test with the child exit status and captured output
while allowing other tests to finish. Expected-success regressions remain active
when they expose library defects; failures are not converted into expected panics.

Reference-model tests use fixed seeds and check entity liveness, component values,
and query counts throughout mixed storage operations. They require neither random
number dependencies nor timing assumptions. Threaded tests assert command delivery
and dependency ordering without asserting the order of independent work.

## Regression contracts

- Reserved IDs accept component and bundle insertion after materialization.
- `spawn_or_insert` replaces supplied component types and preserves other types;
  replacement hooks run, and hook-driven despawn prevents returning a live handle.
- Dynamic builders transfer or destroy each component exactly once, support empty
  and aligned zero-sized bundles, and release their storage on consumption or drop.
- Boolean queries evaluate only matching arms and keep entity and chunk matches
  independent. Per-entity filters retain exact boolean semantics.
- Mutable batches update change epochs for fetched entities.
- Batch iteration visits each touched chunk before fetching, respects exclusive
  range ends, and rejects zero batch sizes before acquiring component borrows.
- World loading preserves present components, updates supplied values, and removes
  components absent from serialized presence masks.

The interaction scenarios assert state at command, scheduler, and flow execution
boundaries. The scheduler lifecycle replays the same five-tick workload through
each enabled runner. Larger world and checkpoint scenarios span multiple storage
chunks and compare component state and relation membership against expected values.
Resources and relations are reconstructed explicitly when loading checkpoints.
Dumps omit entities with none of the selected component types; application-side
commands propagate removal of all selected components for those entities.

## Verification

Windows workspace runs include unit tests, integration tests, and doctests:

| Configuration | Integration tests passed | Failures across workspace |
| --- | ---: | ---: |
| Default features, debug | 128 | 0 |
| All features, debug | 134 | 0 |
| No default features, debug | 118 | 0 |
| All features, release | 134 | 0 |

The 641-entity interaction scenario combines threaded command ingress, archetype
moves, filtered mutable batching, relation cleanup, and exact change detection:

```text
cargo test --test interactions_world threaded_commands_archetype_moves_and_selective_mutation_match_reference_snapshot -- --exact
```

The scheduler-only configuration without default features also passes its
runtime interaction test:

```text
cargo test --test interactions_runtime --no-default-features --features scheduler
```

For the release configuration:

```text
cargo test --workspace --all-features --release --no-fail-fast
```
