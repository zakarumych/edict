# Edict

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

With Edict owner stores strong references ([`Entity`]) to owned entities.
When strong reference is dropped - possibly together with the component on despawn -
the entity will be despawned if no other strong references left.

Edict provides [`WeakEntity`] reference type which works as entity references in traditional ECS.

Another feature of Edict is integrated change detection.
It tracks when components are accessed mutably and may efficiently iterate through modified components.
"Modified when?" Careful reader may inquire.
Imagine a game loop, where a set of systems run on each cycle.
If system has a query over modified components, it probably wants to see all modifications
since it ran this query last time.
Edict offeers [`Tracks`] type. Created simply with [`World::tracks()`],
this type is used in all queries that checks for components modification.
[`Tracks`] instance inform the query, that that modifications occured
since the last use of this [`Tracks`] instance should be returned by query.
On the first use of [`Tracks`] returnd from [`World::tracks()`] all components are considered to be modified.
[`World::tracks_now()`] returns [`Tracks`] instance
for which all modifications happened prior [`World::tracks_now()`] call to be obsolete.

### no_std support

Edict supports `no_std` environment, but requires `alloc`.
With `"std"` feature error types implement `Error` trait,
apart from that only few internal pieces depend on `"std"` feature.
`"std"` feature is enabled by default and must be turned off for `no_std` environemnt.
Dependent crates that also support `no_std` should use `default-features = false` for `edict` dependency,
and optionally enable `"std"` if needed.

[`World`]: https://docs.rs/edict/0.0.1/edict/world/struct.World.html
[`Entity`]: https://docs.rs/edict/0.0.1/edict/entity/struct.Entity.html
[`WeakEntity`]: https://docs.rs/edict/0.0.1/edict/entity/struct.WeakEntity.html
[`Tracks`]: https://docs.rs/edict/0.0.1/edict/tracks/struct.Tracks.html
[`World::tracks()`]: https://docs.rs/edict/0.0.1/edict/world/struct.World.html#method.tracks
[`World::tracks_now()`]: https://docs.rs/edict/0.0.1/edict/world/struct.World.html#method.tracks_now


## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
