# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this 

## [0.7.0]

### Changed

Renames in Flow API
`FlowEntity` -> `Entity`
`FlowWorld` -> `World`

### Fixes

Fixes unsoundness in flows.

## [0.6.0]

**Huge API changes**

### Added

Flow API for writing asynchronous tasks, both local for an entity and global for world.
Flows run on the internal scheduler that can be invoked with system or function call.

Entity API is now extended for both ergonomics and performance.

Entity now can be represented by different types that implement traits from following hierarchy
- `Entity` - must be implemented for all entity types. Provide access to `EntityId` and fallible location lookup.
- `AliveEntity: Entity` - provides guarantee that entity is alive and has non-fallible lookup functionality.
- `LocatedEntity: AliveEntity` - same as above but location is know without access to the `World`.

This hierarchy is implemented by following types out of the box:
- `EntityId` - weakest type of entity. Knows its ID, may be expired already. Implements `Entity`.
  This is the only kind of entity that does not hold lifetime from `World` borrow.
- `EntityBound` - entity know to be alive with lifetime bound to `World` borrow. Implements `AliveEntity`.
  Still requires `World` to lookup its location, so no performance gain here. But allows to use that does not return `NoSuchEntity` error.
  This kind is produced when querying relations. Related entities are guaranteed to be alive, but their location is not known.
- `EntityLoc` - entity know to be alive and contains its location. Implements `LocatedEntity`. Sped up entity's components access.
  Stores location of the entity  (which can't be changed since `World` is borrowed immutably) so it can access components without lookup in ID -> Location mapping.
  This kind is produced by `Entities` query. So accessing other components not in query is faster. It is better to not include `Option<&T>` in query if
  component is rare or when actual use of the component is rare.
  And `Option<&mut T>` is also preventing marking components modified when not needed.
- `EntityRef` - Same as above but also borrows `World` mutably to allow entity modification.
  Produced by `World::spawn` and `World::entity` methods.
  Allows performing entity modifications without looking up entity location each time.
  Note that it is impossible to hold `EntityLoc` to modify entity since modification requires mutable borrow of `World`.

### Changed

Query API. Renamed `QueryRef` to `ViewValue` to better reflect its meaning.
`QueryOneRef` is now `ViewOne`.
Data borrowing kinds goes to type level.

`StaticallyBorrowed` is created when data is borrowed externally - suitable for systems.
`ViewValue<..., StaticallyBorrowed>` has `View` alias.

`RuntimeBorrow` borrows data at runtime.
`ViewValue<..., RuntimeBorrow>` has `ViewCell` alias.

`View` can be created using mutable borrow of `World` and commonly used in systems.
Scheduler ensures that no conflicting `View` instances exist at the same time.
This means that pair of systems with conflicting `View`s will not run in parallel
and system with two conflicting `View`s is invalid and causes panic on scheduling.
`ViewCell` can be created using shared borrow of `World` methods.
In a system two or more `ViewCell` can conflict. User should ensure that they are not used
at the same time. For example with conflicting `a: ViewCell` and `b: ViewCell` in a system
the user can use view `a`, then drop or `unlock` it and then use view `b`.  

Intrasystem conflict resolution:
Static conflict happens when two or more views declare access to the same component
and at least one of them needs write access.
Dynamic conflict happens when two or more views try to borrow same component of the same archetype
and at least one of them needs write access.

`IntoSystem::into_system` will fail with panic if static conflict is detected for `ViewCell`.
Dynamic conflict is detected at runtime for `View` with `RuntimeBorrow` and produces a panic.

Previously all conflicts were resolved at runtime, but they take quite a few precious CPU cycles. 

## [0.3.3] - 2023-01-08

### Added

- public method to create `ActionEncoder` from `ActionBuffer`.

## [0.3.2] - 2023-01-07

### Added

- `Modified<Copied>` query
- `Modified<With>` filter

## [0.3.1] - 2023-01-07

### Added

- `Copied` query to yield component copies instead of references.