

[![crates](https://img.shields.io/crates/v/edict.svg?style=for-the-badge&label=edict)](https://crates.io/crates/edict)
[![docs](https://img.shields.io/badge/docs.rs-edict-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/edict)
[![actions](https://img.shields.io/github/workflow/status/zakarumych/edict/badge/master?style=for-the-badge)](https://github.com/zakarumych/edict/actions?query=workflow%3ARust)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/tokei/lines/github/zakarumych/edict?style=for-the-badge)

## Edict

is experimental ECS with ref-counted entities and built-in change detection
written in Rust by your fellow 🦀

### Features
Counting references to individual entities adds few desirable properties.

For one, strong entity reference guarantees that enitity it referes to is alive.
This allows providing non-fallible API to fetch data attached to entities.

Another one is automatic entity despawn when no one references the entity.
This may seem as a step backward, as most ECS tend to require manual entity despawn,
allowing entities to just sit in the [`World`] and be queried by systems.
This may lead to problems when encoding ownership of an entity by another.
If references to owned entities are stored in owner's component,
then despawning the owner will break the relationship,
code that despawns the owner may be unaware about component that holds refrences to owned entitites.
This will leave previously owned entities orphaned. Hence owned entity must store a reference to its owner
and check periodically if owner was despawned.

With `edict` owner stores strong references ([`Entity`]) to owned entities.
When strong reference is dropped - possibly together with the component on despawn -
the entity will be despawned if no other strong references left.

`edict` provides [`EntityId`] reference type which works as entity references in traditional ECS.

Another feature of `edict` is integrated change detection.
It tracks when components are accessed mutably and may efficiently iterate through modified components.
"Modified when?" Careful reader may inquire.
Imagine a game loop, where a set of systems run on each cycle.
If system has a query over modified components, it probably wants to see all modifications
since it ran this query last time.
`edict` offeers [`Tracks`] type. Created simply with [`World::tracks()`],
this type is used in all queries that checks for components modification.
[`Tracks`] instance inform the query, that that modifications occured
since the last use of this [`Tracks`] instance should be returned by query.
On the first use of [`Tracks`] returnd from [`World::tracks()`] all components are considered to be modified.
[`World::tracks_now()`] returns [`Tracks`] instance
for which all modifications happened prior [`World::tracks_now()`] call to be obsolete.

### no_std support

`edict` supports `no_std` environment, but requires `alloc`.
With `"std"` feature error types implement `Error` trait,
apart from that only few internal pieces depend on `"std"` feature.
`"std"` feature is enabled by default and must be turned off for `no_std` environemnt.
Dependent crates that also support `no_std` should use `default-features = false` for `edict` dependency,
and optionally enable `"std"` if needed.

[`World`]: https://docs.rs/edict/0.0.2/edict/world/struct.World.html
[`Entity`]: https://docs.rs/edict/0.0.2/edict/entity/struct.Entity.html
[`EntityId`]: https://docs.rs/edict/0.0.2/edict/entity/struct.EntityId.html
[`Tracks`]: https://docs.rs/edict/0.0.2/edict/tracks/struct.Tracks.html
[`World::tracks()`]: https://docs.rs/edict/0.0.2/edict/world/struct.World.html#method.tracks
[`World::tracks_now()`]: https://docs.rs/edict/0.0.2/edict/world/struct.World.html#method.tracks_now


### Benching

Results gathered from [`ecs_bench_suite`](https://github.com/rust-gamedev/ecs_bench_suite)
Tested on Apple's M1 cpu.
100% is the fastest one in the column.

| ECS           | simple_insert     | simple_iter      | fragmented_iter   | add_remove_component
----------------|-------------------|------------------|-------------------|----------------------
| legion        | 169.49 us [100%]  | 7.3514 us [106%] | 259.69 ns [521%]  | 1.9975 ms [3047%]
| hecs          | 367.39 us [217%]  | 6.9587 us [100%] | 451.25 ns [905%]  | 589.64 us [1112%]
| bevy          | 414.22 us [244%]  | 8.8831 us [128%] | 1.2211 us [2448%] | 1.4385 ms [2194%]
| planck_ecs    | 335.36 us [198%]  | 24.519 us [352%] | 517.86 ns [1038%] | 67.039 us [102%]
| shipyard      | 324.81 us [192%]  | 15.837 us [227%] | 49.883 ns [100%]  | 65.877 us [100%]
| specs         | 875.03 us [516%]  | 34.117 us [490%] | 1.8565 us [3722%] | 65.551 us [100%]
| edict         | 697.68 us [412%]  | 7.5105 us [108%] | 170.77 ns [342%]  | 395.12 us [603%]

#### Analysis

`edict` suffers expected penalty on entity spawn in `simple_insert`, it has to allocate storage for refcounts. There's room for improvement.

Iteration is relatively similar to `hecs` (the fastest in `simple_iter`), although `edict` tracks modifications, while `hecs` doesn't.

In `fragmented_iter` benchmark `edict` shows its low perf cost for iteration on each archetype and so performs great iterating on many archetypes with fewer entities.

In `add_remove_component` benchmark `edict` performs surprisingly better than `hecs`. `legion` and `bevy` can't keep up.
Sparse set based ECSs are unchallenged winners here.

## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
