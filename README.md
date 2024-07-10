
[![crates](https://img.shields.io/crates/v/edict.svg?style=for-the-badge&label=edict)](https://crates.io/crates/edict)
[![docs](https://img.shields.io/badge/docs.rs-edict-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/edict)
[![actions](https://img.shields.io/github/actions/workflow/status/zakarumych/edict/badge.yml?branch=main&style=for-the-badge)](https://github.com/zakarumych/edict/actions/workflows/badge.yml)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/endpoint?url=https://ghloc.vercel.app/api/zakarumych/edict/badge?filter=.py$,.scss$,.rs$&style=for-the-badge&logoColor=white&label=Lines%20of%20Code)
Edict is a fast, powerful and ergonomic ECS crate that expands traditional ECS feature set.
Written in Rust by your fellow 🦀

# Basic usage 🌱

```rust
use edict::prelude::*;

// Create world instance.
let mut world = World::new();

// Declare some components.
#[derive(Component)]
struct Pos(f32, f32);

// Declare some more.
#[derive(Component)]
struct Vel(f32, f32);

// Spawn entity with components.
world.spawn((Pos(0.0, 0.0), Vel(1.0, 1.0)));

// Query components and iterate over views.
for (pos, vel) in world.view::<(&mut Pos, &Vel)>() {
  pos.0 += vel.0;
  pos.1 += vel.1;
}


// Define functions that will be used as systems.
#[edict::system::system] // This attribute is optional, but it catches if function is not a system.
fn move_system(pos_vel: View<(&mut Pos, &Vel)>) {
  for (pos, vel) in pos_vel {
    pos.0 += vel.0;
    pos.1 += vel.1;
  }
}

// Create scheduler to run systems. Requires "scheduler" feature.
use edict::scheduler::Scheduler;

let mut scheduler = Scheduler::new();
scheduler.add_system(move_system);

// Run systems without parallelism.
scheduler.run_sequential(&mut world);

// Run systems using threads. Requires "std" feature.
scheduler.run_threaded(&mut world);

// Or use custom thread pool.
```

# Features

## Entities 🧩

### Simple IDs

In *Entity* Component Systems we create entities and address them to fetch associated data.
Edict provides [`EntityId`] type to address entities.

[`EntityId`] as a world-unique identifier of an entity.
Edict uses IDs without generation and recycling, for this purpose it employs `u64` underlying type with a few niches.
It is enough to create IDs non-stop for hundreds of years before running out of them.
This greatly simplifying serialization of the [`World`]'s state as it doesn't require any processing of entity IDs.

By default entity IDs are unique only within one [`World`].
For multi-world scenarios Edict provides a way to make entity IDs unique between any required combination of worlds.

IDs are allocated in sequence from [`IdRange`]s that are allocated by [`IdRangeAllocator`].
By default [`IdRange`] that spans from 1 to `u64::MAX - 1` is used. This makes default ID allocation extremely fast.
Custom [`IdRangeAllocator`] can be provided to [`WorldBuilder`] to use custom ID ranges.

For example in client-server architecture, server and client may use non-overlapping ID ranges.
Thus allowing state serialized on server to be transferred to client without ID mapping,
which can be cumbersome when components reference entities.

In multi-server or p2p architecture [`IdRangeAllocator`] would need to communicate to allocate disjoint ID ranges for each server.

### Ergonomic entity types

Using ECS may lead to lots of `.unwrap()` calls or excessive error handling.
There a lot of situations when entity is guaranteed to exist (for example it just returned from a view).
To avoid handling [`NoSuchEntity`] error when it is unreachable, Edict provides [`AliveEntity`] trait that extends [`Entity`] trait.
Various methods require [`AliveEntity`] handle and skip existence check.

[`Entity`] and [`AliveEntity`] traits implemented for number of entity types.

[`EntityId`] implements only [`Entity`] as it doesn't provide any guaranties.

[`EntityBound`] is guaranteed to be alive, allowing using it in methods that doesn't handle entity absence.
It keeps lifetime of [`World`] borrow, making it impossible to despawn any entity from the world.
Using it with wrong [`World`] may cause panic.
[`EntityBound`] can be acquire from relation queries.

[`EntityLoc`] not only guarantees entity existence but also contains location of the entity in the archetypes,
allowing to skip lookup step when accessing its components.
Similarly to [`EntityBound`], it keeps lifetime of [`World`] borrow, making it impossible to despawn any entity from the world.
Using it with wrong [`World`] may cause panic.
[`EntityLoc`] can be acquire from [`Entities`] query.

[`EntityRef`] is special.
It doesn't implement [`Entity`] or [`AliveEntity`] traits since it should not be used in world methods.
Instead it provides direct access to entity's data and allows mutations such as inserting/removing components.

## Components 🛠️

### Non-thread-safe types

Support for [`!Send`] and [`!Sync`] components and resources with some limitations.

[`World`] itself is not sendable but shareable between threads.
Thread owning [`World`] is referred as "main" thread in documentation.

Components and resources that are [`!Send`] can be fetched mutably only from "main" thread.
Components and resources that are [`!Sync`] can be fetched immutably only from "main" thread.
Since reference to [`World`] may exist outside "main" thread, [`WorldLocal`] reference should be used,
it can be created using mutable reference to [`World`].

### Components with trait and without

Optional [`Component`] trait that allows implicit component type registration when component is inserted first time.
Implicit registration uses behavior defined by [`Component`] implementation as-is.
When needed, explicit registration can be done using [`WorldBuilder`] to override component behavior.

Non [`Component`] types require explicit registration and
few methods with `_external` suffix is used with them instead of normal ones.
Only default registration is possible when [`World`] is already built.
When needed, explicit registration can be done using [`WorldBuilder`] to override component behavior.

## Entity relations 🔗

A relation can be added to pair of entities, binding them together.
Queries may fetch relations and filter entities by their relations to other entities.
When either of the two entities is despawned, relation is dropped.
[`Relation`] type may further configure behavior of the bounded entities.

## Queries 🔍

Powerful [`Query`] mechanism that can filter entities by components, relations and other criteria and fetch entity data.
Queries can be mutable or immutable, sendable or non-sendable, stateful or stateless.

Using query on [`World`] creates Views.
Views can be used to iterate over entities that match the query yielding query items.
Or fetch single entity data.

[`ViewRef`] and [`ViewMut`] are convenient type aliases to view types returned from [`World`] methods.

## Runtime and compile time checks

Runtime checks are available for query mutable aliasing avoidance.

[`ViewRef`] and [`ViewCell`] do runtime checks allowing multiple views with aliased access coexist,
deferring checks to runtime that prevents invalid aliasing to occur.

When this is not required, [`ViewMut`] and [`View`]s with compile time checks should be used instead.

When [`View`] is expected [`ViewRef`] and [`ViewCell`] can be locked to make a [`View`].

### Borrows

Component type may define borrowing operations to borrow another type from it.
Borrowed type may be not sized, allowing slices and dyn traits to be borrowed.
A macro to help define borrowing operations is provided.
Queries that tries to borrow type from suitable components are provided:
* [`BorrowAll`] borrows from all components that implement borrowing requested type.
  Yields a [`Vec`] with borrowed values since multiple components of the entity may provide it.
  Skips entities if none of the components provide the requested type.
* [`BorrowAny`] borrows from first suitable component that implements borrowing requested type.
  Yields a single value.
  Skips entities if none of the components provide the requested type.
* [`BorrowOne`] is configured with [`TypeId`] of component from which it should borrow requested type.
  Panics if component doesn't provide the requested type.
  Skips entities without the component.

## Resources 📦

Built-in type-map for singleton values called "resources".
Resources can be inserted into/fetched from [`World`].
Resources live separately from entities and their components.

## Actions 🏃‍♂️

Use [`ActionEncoder`] for recording actions and run them later with mutable access to [`World`].
Or [`LocalActionEncoder`] instead when action is not [`Send`].
Or convenient [`WorldLocal::defer*`] methods to defer actions to internal [`LocalActionEncoder`].

## Automatic change tracking 🤖

Each component instance is equipped with epoch counter that tracks last potential mutation of the component.
Queries may read and update components epoch to track changes.
Queries to filter recently changed components are provided with [`Modified`] type.
Last epoch can be obtained with [`World::epoch`].

## Systems ⚙️

Systems is convenient way to build logic that operates on [`World`].
Edict defines [`System`] trait to run logic on [`World`].
And [`IntoSystem`] trait for types convertible to [`System`].

Functions may implement [`IntoSystem`] automatically -
it is required to return `()` and accept arguments that implement [`FnArg`] trait.
There are [`FnArg`] implementations:

- [`View`] and [`ViewCell`] to iterate over entities and their components.
  Use [`View`] unless [`ViewCell`] is required to handle intra-system views conflict.
- [`Res`] and [`ResMut`] to access resources.
- [`ResLocal`] and [`ResMutLocal`] to access no-thread-safe resources.
  This will make system non-sendable and force it to run on main thread.
- [`ActionEncoder`] to record actions that mutate [`World`] state, such as entity spawning, inserting and removing components or resources.
- [`State`] to store system's local state between runs.

## Easy scheduler 📅

[`Scheduler`] is provided to run [`System`]s.
Systems added to the [`Scheduler`] run in parallel where possible,
however they act **as if** executed sequentially in order they were added.

If systems do not conflict they may be executed in parallel.

If systems conflict, the one added first will be executed before the one added later can start.

`std` threads or `rayon` can be used as an executor.
User may provide custom executor by implementing [`ScopedExecutor`] trait.

Requires `"scheduler"` feature which is enabled by default.

## Hooks 🎣

Component replace/drop hooks are called automatically when component is replaced or dropped.

When component is registered it can be equipped with hooks to be called when component value is replaced or dropped.
Implicit registration of [`Component`] types will register hooks defined on the trait impl.

Drop hook is called when component is dropped via [`World::drop`] or entity is despawned and is not
called when component is removed from entity.

Replace hook is called when component is replaced e.g. component is inserted into entity
and entity already has component of the same type.
Replace hook returns boolean value that indicates if drop hook should be called for replaced component.

Hooks can record actions into provided [`LocalActionEncoder`] that will be executed
before [`World`] method that caused the hook to be called returns.

When component implements [`Component`] trait, hooks defined on the trait impl are registered automatically to call
[`Component::on_drop`] and [`Component::on_replace`] methods.
They may be overridden with custom hooks using [`WorldBuilder`].
For non [`Component`] types hooks can be registered only via [`WorldBuilder`].
Default registration with [`World`] will not register any hooks.

## Async-await ⏳

Futures executor to run logic that requires waiting for certain conditions or events
or otherwise spans for multiple ticks.

Logic that requires waiting can be complex to implement using systems.
Systems run in loop and usually work on every entity with certain components.
Implementing waiting logic would require adding waiting state to existing or new components and
logic would be spread across many system runs or even many systems.

Futures may use `await` syntax to wait for certain conditions or events.
Futures that can access ECS data are referred in Edict as "flows".

Flows can be spawned in the [`World`] using [`World::spawn_flow`] or [`FlowWorld::spawn_flow`] method.
[`Flows`] type is used as an executor to run spawned flows.

Flows can be bound to an entity and spawned using [`World::spawn_flow_for`], [`FlowWorld::spawn_flow_for`], [`EntityRef::spawn_flow`] or [`FlowEntity::spawn_flow`] method.
Such flows will be cancelled if entity is despawned.

Functions that return futures may serve as flows.
For [`World::spawn_flow`] use function or closure with signature `FnOnce(FlowWorld) -> Future`
For [`World::spawn_flow_for`] use function or closure with signature `FnOnce(FlowEntity) -> Future`

User may implement low-level futures using `poll*` methods of [`FlowWorld`] and [`FlowEntity`] to access tasks [`Context`].
Edict provides only a couple of low-level futures that will do the waiting:
- [`yield_now!`] yields control to the executor once and resumes on next execution.
- [`FlowEntity::wait_despawned`] waits until entity is despawned.
- [`FlowEntity::wait_has_component`] waits until entity get a component.

[`WakeOnDrop`] component can be used when despawning entity should wake a task.

It is recommended to use flows for high-level logic that spans multiple ticks
and use systems to do low-level logic that runs every tick.
Flows may request systems to perform operations by adding special components to entities.
And systems may spawn flows to do long-running operations.

Requires `"flow"` feature which is enabled by default.

# no_std support

Edict can be used in `no_std` environment but requires `alloc` crate.
`"std"` feature is enabled by default.

If "std" feature is not enabled error types will not implement [`std::error::Error`].

When "flow" feature is enabled and "std" is not, extern functions are used to implement TLS.
Application must provide implementation for these functions or linking will fail.

When "scheduler" feature is enabled and "std" is not, external functions are used to implement thread parking.
Application must provide implementation for these functions or linking will fail.

[`!Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
[`!Sized`]: https://doc.rust-lang.org/std/marker/trait.Sized.html
[`!Sync`]: https://doc.rust-lang.org/std/marker/trait.Sync.html
[`ActionEncoder`]: https://docs.rs/edict/1.0.0-rc4/edict/action/struct.ActionEncoder.html
[`AliveEntity`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/trait.AliveEntity.html
[`BorrowAll`]: https://docs.rs/edict/1.0.0-rc4/edict/query/struct.BorrowAll.html
[`BorrowAny`]: https://docs.rs/edict/1.0.0-rc4/edict/query/struct.BorrowAny.html
[`BorrowOne`]: https://docs.rs/edict/1.0.0-rc4/edict/query/struct.BorrowOne.html
[`Component`]: https://docs.rs/edict/1.0.0-rc4/edict/component/trait.Component.html
[`Component::on_drop`]: https://docs.rs/edict/1.0.0-rc4/edict/component/trait.Component.html#method.on_drop
[`Component::on_replace`]: https://docs.rs/edict/1.0.0-rc4/edict/component/trait.Component.html#method.on_replace
[`Context`]: https://doc.rust-lang.org/std/task/struct.Context.html
[`Entities`]: https://docs.rs/edict/1.0.0-rc4/edict/query/struct.Entities.html
[`Entity`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/trait.Entity.html
[`EntityBound`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/trait.EntityBound.html
[`EntityId`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/struct.EntityId.html
[`EntityLoc`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/struct.EntityLoc.html
[`EntityRef`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/struct.EntityRef.html
[`EntityRef::spawn_flow`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/struct.EntityRef.html#method.spawn_flow
[`flow`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/index.html
[`FlowEntity`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowEntity.html
[`FlowEntity::spawn_flow`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowEntity.html#method.spawn_flow
[`FlowEntity::wait_despawned`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowEntity.html#method.wait_despawned
[`FlowEntity::wait_has_component`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowEntity.html#method.wait_has_component
[`Flows`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.Flows.html
[`Flows::execute`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.Flows.html#method.execute
[`FlowWorld`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowWorld.html
[`FlowWorld::spawn_flow`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowWorld.html#method.spawn_flow
[`FlowWorld::spawn_flow_for`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.FlowWorld.html#method.spawn_flow_for
[`FnArg`]: https://docs.rs/edict/1.0.0-rc4/edict/system/trait.FnArg.html
[`IdRange`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/struct.IdRange.html
[`IdRangeAllocator`]: https://docs.rs/edict/1.0.0-rc4/edict/entity/trait.IdRangeAllocator.html
[`IntoSystem`]: https://docs.rs/edict/1.0.0-rc4/edict/system/trait.IntoSystem.html
[`LocalActionEncoder`]: https://docs.rs/edict/1.0.0-rc4/edict/action/struct.LocalActionEncoder.html
[`Modified`]: https://docs.rs/edict/1.0.0-rc4/edict/query/struct.Modified.html
[`NoSuchEntity`]: https://docs.rs/edict/1.0.0-rc4/edict/struct.NoSuchEntity.html
[`Query`]: https://docs.rs/edict/1.0.0-rc4/edict/query/trait.Query.html
[`Relation`]: https://docs.rs/edict/1.0.0-rc4/edict/relation/trait.Relation.html
[`Res`]: https://docs.rs/edict/1.0.0-rc4/edict/resources/struct.Res.html
[`ResMut`]: https://docs.rs/edict/1.0.0-rc4/edict/resources/struct.ResMut.html
[`ResLocal`]: https://docs.rs/edict/1.0.0-rc4/edict/system/struct.ResLocal.html
[`ResMutLocal`]: https://docs.rs/edict/1.0.0-rc4/edict/system/struct.ResMutLocal.html
[`Scheduler`]: https://docs.rs/edict/1.0.0-rc4/edict/scheduler/struct.Scheduler.html
[`ScopedExecutor`]: https://docs.rs/edict/1.0.0-rc4/edict/executor/trait.ScopedExecutor.html
[`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
[`Sized`]: https://doc.rust-lang.org/std/marker/trait.Sized.html
[`State`]: https://docs.rs/edict/1.0.0-rc4/edict/system/struct.State.html
[`std::error::Error`]: https://doc.rust-lang.org/std/error/trait.Error.html
[`Sync`]: https://doc.rust-lang.org/std/marker/trait.Sync.html
[`System`]: https://docs.rs/edict/1.0.0-rc4/edict/system/trait.System.html
[`TypeId`]: https://doc.rust-lang.org/std/any/struct.TypeId.html
[`Vec`]: https://doc.rust-lang.org/std/vec/struct.Vec.html
[`View`]: https://docs.rs/edict/1.0.0-rc4/edict/view/type.View.html
[`ViewCell`]: https://docs.rs/edict/1.0.0-rc4/edict/view/type.ViewCell.html
[`ViewMut`]: https://docs.rs/edict/1.0.0-rc4/edict/view/type.ViewMut.html
[`ViewRef`]: https://docs.rs/edict/1.0.0-rc4/edict/view/type.ViewRef.html
[`WakeOnDrop`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/struct.WakeOnDrop.html
[`World`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.World.html
[`World::drop`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.World.html#method.drop
[`World::epoch`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.World.html#method.epoch
[`World::spawn_flow`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.World.html#method.spawn_flow
[`World::spawn_flow_for`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.World.html#method.spawn_flow_for
[`WorldBuilder`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.WorldBuilder.html
[`WorldLocal`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.WorldLocal.html
[`WorldLocal::defer*`]: https://docs.rs/edict/1.0.0-rc4/edict/world/struct.WorldLocal.html#method.defer
[`yield_now!`]: https://docs.rs/edict/1.0.0-rc4/edict/flow/macro.yield_now.html

## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
