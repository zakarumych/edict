
[![crates](https://img.shields.io/crates/v/edict.svg?style=for-the-badge&label=edict)](https://crates.io/crates/edict)
[![docs](https://img.shields.io/badge/docs.rs-edict-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/edict)
[![actions](https://img.shields.io/github/actions/workflow/status/zakarumych/edict/badge.yml?branch=master&style=for-the-badge)](https://github.com/zakarumych/edict/actions/workflows/badge.yml)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/tokei/lines/github/zakarumych/edict?style=for-the-badge)

## Edict

Edict is a fast and powerful ECS crate that expands traditional ECS feature set.
Written in Rust by your fellow 🦀

### Features

* General purpose archetype based ECS with fast iteration.

* Relations can be added to pair of entities, binding them together.
  When either of the two entities is despawned, relation is dropped.
  [`Relation`] type may further configure behavior of the bonds.

* Change tracking.
  Each component instance is equipped with epoch counter that tracks last potential mutation of the component.
  Special query type uses epoch counter to skip entities where component wasn't changed since specified epoch.
  Last epoch can be obtained with [`World::epoch`].

* Built-in type-map for singleton values called "resources".
  Resources can be inserted into/fetched from [`World`].
  Resources live separately from entities and their components.

* Runtime checks for query validity and mutable aliasing avoidance.
  This requires atomic operations at the beginning iteration on next archetype.

* Support for [`!Send`] and [`!Sync`] components.
  [`!Send`] components cannot be fetched mutably from outside "main" thread.
  [`!Sync`] components cannot be fetched immutably from outside "main" thread.
  [`World`] has to be [`!Send`] but implements [`Sync`].

* [`ActionEncoder`] allows recording actions and later run them on [`World`].
  Actions get mutable access to [`World`].

* Component replace/drop hooks.
  Components can define hooks that will be executed on value drop and replace.
  Hooks can read old and new values, [`EntityId`] and can record actions into [`ActionEncoder`].

* Component type may define a set of types that can be borrowed from it.
  Borrowed type may be not sized, allowing slices, dyn traits and any other [`!Sized`] types.
  There's macro to define dyn trait borrows.
  Special kind of queries look into possible borrows to fetch.

* [`WorldBuilder`] can be used to manually register component types and override default behavior.

* Optional [`Component`] trait to allow implicit component type registration by insertion methods.
  Implicit registration uses behavior defined by [`Component`] implementation as-is.
  Separate insertions methods with [`Component`] trait bound lifted can be used where trait is not implemented or implementation is not visible for generic type.
  Those methods require pre-registration of the component type. If type was not registered - method panics.
  Both explicit registration with [`WorldBuilder`] and implicit registration via insertion method with [`Component`] type bound is enough.

* [`System`] trait and [`IntoSystem`] implemented for functions if argument types implement [`FnArg`].
  This way practically any system can be defined as a function.

* [`Scheduler`] that can run [`System`]s in parallel using provided executor.

### no_std support

`edict` can be used in `no_std` environment but requires `alloc`.
With `"std"` feature enabled error types implement `Error` trait.
`"std"` feature is enabled by default and must be turned off for `no_std` environment.
Dependent crates that also support `no_std` should use `default-features = false` for `edict` dependency,
and optionally enable `"std"` if needed.

[`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
[`!Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
[`Sync`]: https://doc.rust-lang.org/std/marker/trait.Sync.html
[`!Sync`]: https://doc.rust-lang.org/std/marker/trait.Sync.html
[`World`]: https://docs.rs/edict/latest/edict/world/struct.World.html
[`ActionEncoder`]: https://docs.rs/edict/latest/edict/action/struct.ActionEncoder.html
[`EntityId`]: https://docs.rs/edict/latest/edict/entity/struct.EntityId.html
[`!Sized`]: https://doc.rust-lang.org/std/marker/trait.Sized.html
[`Component`]: https://docs.rs/edict/latest/edict/component/struct.Component.html
[`World::epoch`]: https://docs.rs/edict/latest/edict/world/struct.World.html#method.epoch
[`Relation`]: https://docs.rs/edict/latest/edict/relation/trait.Relation.html
[`WorldBuilder`]: https://docs.rs/edict/latest/edict/world/struct.WorldBuilder.html
[`System`]: https://docs.rs/edict/latest/edict/system/struct.System.html
[`IntoSystem`]: https://docs.rs/edict/latest/edict/system/struct.IntoSystem.html
[`FnArg`]: https://docs.rs/edict/latest/edict/system/struct.FnArg.html
[`Scheduler`]: https://docs.rs/edict/latest/edict/scheduler/struct.Scheduler.html

## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
