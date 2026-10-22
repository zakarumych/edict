# Runnable ECS showcases

Run these from the repository root. Every example uses fixed inputs, finishes
without interaction, prints state derived from the ECS, and checks its own
invariants. No extra dependencies, assets, network services, or output files are
needed.

## Scene graph

```text
cargo run --example scene_graph
```

An observatory carries a telescope arm, lens, and beacon. Rotation and translation
propagate through `ChildOf` ancestry. The telescope moves onto a rover, then the
observatory and rover are removed in turn. Printed trees show the inherited poses
and which subtrees survive each operation.

This combines dynamic bundles, exclusive owned relations, hierarchy traversal,
and `Alt` change detection. Reparenting preserves the local pose. An unchanged
propagation pass leaves world-pose change epochs untouched.

## Particle fountain

```text
cargo run --example particle_fountain
```

A fountain emits 1,536 particles in bursts. Integer gravity and lifetimes drive
the motion; ASCII density frames display the component state. Emission stops,
the fountain drains, and the final accounting proves that every emitted particle
has retired.

This combines batch spawning, mutable and `Alt` queries, resources, change epochs,
and deferred destruction. Despawns execute after the simulation view releases its
borrows. Frames are printed consecutively, so captured terminal output remains
readable.

## Colony

```text
cargo run --example colony
```

A scheduled colony manages production and construction over successive ticks.
The tick table connects resource consumption to production and expansion;
relations and component hooks handle building ownership and cleanup.

Requires the `scheduler` and `std` features, both enabled by default.

## Async factory

```text
cargo run --example async_factory
```

Orders progress through async workflows while ECS systems perform the work.
The event timeline shows the flow executor and scheduler handing control to each
other. Removing an owner cancels unfinished work and exercises cleanup hooks.

Requires `flow`, `scheduler`, and `std`, all enabled by default. Execution uses
explicit ticks and wakeups, with no wall-clock delays or external async runtime.

## Checkpoint and replay

```text
cargo run --example checkpoint_replay --features nanoserde
```

Serialize a binary checkpoint, advance the source world, apply a delta to a
restored world, and resume simulation. Printed byte counts and state checksums
show the transfer and the agreement between both timelines.

Resources are checkpointed explicitly and a load marker reconstructs owned
relations. Component dumps do not automatically serialize resources, relations,
or entity-despawn records. The example keeps those responsibilities visible.
Requires `nanoserde` and `std`.

## Live workshop

```text
cargo run --example live_workshop
```

A producer thread submits scripted world edits through `ActionSender`. The main
thread applies the edits and prints an inspector of heterogeneous components.
Dynamic bundles, trait borrowing, dirty-state queries, and owned cleanup combine
into a small live-editing workflow.

Thread synchronization defines when edits are visible; there are no timing-based
assumptions. The world stays on its owning thread.

## Feature configurations

Scene graph, particle fountain, and live workshop also run with
`--no-default-features`: their host binaries use the standard library while edict
uses its minimal configuration. Cargo skips feature-dependent examples when
their required features are absent.

To compile every example with all optional ECS features:

```text
cargo check --examples --all-features
```
