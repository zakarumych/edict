---
name: edict-api
description: >
  Complete public API map of the edict crate (this repo, C:\code\edict) — a
  Rust ECS with archetype storage, a Query trait family, entity relations,
  views, systems/scheduler, deferred actions, and async flows. Load this
  before writing or reviewing any code that touches src/world, src/query,
  src/entity, src/component, src/bundle, src/relation, src/view,
  src/system, src/scheduler, src/action, src/flow, or src/dump — it gives
  type signatures, file:line locations, and "what to use when" guidance so
  you don't have to re-grep the source.
  Trigger: working on edict itself (not just using it as a dependency),
  "how does edict's Query trait work", "what entity handle type do I want",
  "add a system", "defer a world mutation", "add a relation", "write a
  flow", questions about World/View/Archetype/Component/Bundle API shape.
---

# edict API map

edict is a Rust ECS crate. This file is a digest of its **public API surface**,
built by reading the source directly (not the README, which can drift). Every
entry has a `file:line` pointer — spot-check the line if you're about to rely
on exact behavior, since line numbers shift as the code changes. The `query/`
module was refactored recently (commits "Simplify query trait family", "Fix
DefaultQuery impls", "Refactor query subtraits") — treat that section as the
most likely to have moved since this digest was written.

## Mental model

- `World` owns entities, their components (stored per-**archetype** — one
  contiguous storage block per unique component-type-set), singleton
  **resources**, and relation data.
- You get data out via a **`Query`** (e.g. `&T`, `&mut T`, tuples of those,
  filters like `With<T>`). Running a query against `World` produces a
  **`View`**, which you iterate or index by entity.
- **Systems** are plain functions whose parameters implement `FnArg`
  (`View`/`ViewCell`, `Res`/`ResMut`, `ActionEncoder`, `State<T>`, …); a
  blanket impl turns them into `IntoSystem` automatically. A `Scheduler`
  runs a set of systems, parallelizing non-conflicting ones.
- **Actions** (`ActionEncoder` etc.) record `World` mutations to apply
  later — use them to mutate `World` while you only have shared access to it
  (inside a system, inside a component/relation hook).
- **Relations** (`Relation` trait, e.g. `ChildOf`) bind two entities
  together and are queried with `Relates`/`Related`/`RelatesTo` and their
  filter counterparts.
- **Flows** are async tasks with implicit `World` access (`FlowWorld`/
  `FlowEntity`), for logic that spans multiple ticks (`unit.move_to(x).await`
  style), driven by a `Flows` executor.
- Five **entity handle types** (`EntityId`, `EntityBound`, `EntityLoc`,
  `EntityRef`, plus low-level `IdRange`/`IdRangeAllocator`) trade off
  alive-guarantee / cached-location / mutability — see table below.

## "What do I use when" cheat sheet

| I need to... | Use |
|---|---|
| Iterate entities with some components | `World::view::<(&A, &mut B)>()` or a system arg `View<(&A, &mut B)>` |
| Fetch/mutate one known-alive entity's components | `View::get`/`get_mut`, or `World::view_one::<Q>(entity)` |
| Read a component by value cheaply (small `Copy` type) | `Cpy<T>` query instead of `&T` |
| Read/write without triggering false-positive change detection on mere access | `Alt<T>` (bumps epoch only on actual `DerefMut`) |
| Filter to entities changed since last check | `Modified<&T>` / `Modified<&mut T>` / `Modified<Alt<T>>`, with `after_epoch` |
| Filter by presence/absence without fetching data | `With<T>` / `Without<T>` (= `Not<With<T>>`) |
| Combine multiple optional matches (any/all/exactly-one of) | `And<T>`/`Or<T>`/`Xor<T>` (`BooleanQuery`) — **not** `AnyOf`/`one_of!`, those are dead/unreachable right now, see Known gaps |
| Get entity identity/location alongside data | `Entities` query (yields `EntityLoc`) |
| A singleton value (config, counters, etc.) | `Res<T>`/`ResMut<T>` (Sync/Send); `ResLocal`/`ResMutLocal` for `!Sync`/`!Send` types (forces system to main thread) |
| Bind two entities together | `Relation` trait + `World::insert_relation`; query with `Relates<...>`/`Related<...>`/`RelatesTo<R>` |
| Parent/child with auto-despawn-of-children | Built-in `ChildOf` relation (`EXCLUSIVE`+`OWNED`) |
| Mutate `World` while you only have `&World` / are inside a hook | `ActionEncoder` (system arg) or `LocalActionEncoder` (hooks) — record, don't mutate directly |
| Mutate `World` from another thread / outside the scheduler | `World::new_action_sender() -> ActionSender`, then `World::execute_received_actions()` |
| Spawn N entities at once efficiently | `World::spawn_batch(iter_of_bundles)` |
| Group several components into one insert/spawn call | `EntityBuilder` (dynamic/runtime shape) or a tuple/`impl Bundle` (static shape) |
| Write a system | plain `fn`/closure whose args all implement `FnArg`; add `#[edict::system]` for better compile errors; `Scheduler::add_system(it)` |
| Logic that spans multiple ticks / waits on a condition | a "flow": `async fn(world: FlowWorld)` or `async fn(e: FlowEntity)`, spawned via `World::spawn_flow`/`spawn_flow_for` |
| Serialize/deserialize world state | `dump` module (`WorldDump`/`WorldLoad`) — secondary feature, pick a backend (`serde`/`nanoserde`/`alkahest` features) |

## Entity handle family (`src/entity/`)

| Type | Alive guaranteed? | Location cached? | Borrows `World`? | Typical source |
|---|---|---|---|---|
| `EntityId` | no | no | no | stored long-term anywhere (e.g. inside components as a foreign-entity link) |
| `EntityBound<'a>` | yes (by lifetime) | no | immutable | produced by queries as cheap alive-proof without location lookup |
| `EntityLoc<'a>` | yes | yes | immutable | produced by queries/`allocate()` that already resolved location |
| `EntityRef<'a>` | yes | yes | **mutable** (of `WorldLocal`) | `World::spawn*`, `EntityId::entity_ref`, `World::entity()` — lets you insert/remove components directly |
| `IdRange`/`IdRangeAllocator` | n/a | n/a | n/a | low-level ID-space allocation, not a per-entity handle |

`Entity` trait (`entity/mod.rs:27`): base abstraction (`id()`, `lookup()`,
`is_alive()`, `entity_loc()`, `entity_ref()`), fallible. `AliveEntity: Entity`
(`entity/mod.rs:47`): non-fallible refinement implemented by `EntityBound`/
`EntityLoc`. Gotcha: the "alive" guarantee on `EntityBound`/`EntityLoc` is a
type-system convention backed by holding a `World` borrow for the value's
lifetime, not a runtime check — mixing entities from different `World`s and
calling `locate`/`entity_loc` **panics**.

### `EntityRef<'a>` (`entity/mod.rs:412`) — the mutation-capable handle
Most-used methods (all take/return `Self` because hooks may despawn the
entity, invalidating the old location — that's why insert/remove return
`Option<Self>`):
- `get`/`get_mut`/`get_with`/`get_with_mut` — query fetch on this entity (`:496-576`)
- `insert::<T: Component>(self, v) -> Option<Self>` / `insert_external::<T>` (`:677`, `:724`)
- `with::<T: Component>(&mut self, f: FnOnce()->T) -> &mut T` — get-or-insert, never despawns (`:754`)
- `insert_bundle`/`insert_external_bundle`/`with_bundle`/`with_external_bundle` (`:837-983`)
- `remove::<T>(&mut self) -> Option<T>` (`:1003`), `drop::<T>(self) -> Option<Self>` (`:1019`), `drop_bundle::<B: Bundle>(self)` (`:1091`)
- `despawn(self)` (`:1115`), `has_component::<T>(&self) -> bool` (`:1123`), `world(&mut self) -> &mut World` (`:1134`)

### `EntitySet` (`entity/set.rs:53`) — id → `Location` table + allocator
Normally reached via `World`, not directly. `Location { arch: u32, idx: u32 }`
(`set.rs:18`) — `arch==0` = empty archetype, `arch==u32::MAX` = reserved/not
yet spawned. `alloc()` (`:136`) reserves an id thread-safely without placing
it in an archetype yet; must later be flushed via `spawn_allocated` — all
`alloc()`-reserved entities must be spawned before any *other* entity, or it
**panics**.

### ID allocation (`entity/allocator.rs`)
`IdRangeAllocator` (unsafe trait, `:203`) — pluggable strategy for handing
`World` blocks of unique ids (e.g. disjoint client/server ranges).
`OneRangeAllocator::client()` (`1..2^48`) / `::server()` (`2^48..u64::MAX-1`)
(`:257`, `:273`) are the built-in presets — doc warns `::server()` is only
safe for a single isolated server; interconnected servers need a custom
allocator drawing from a shared pool.

## Components & Bundles (`src/component.rs`, `src/bundle.rs`)

### `Component` trait (`component.rs:378`)
```rust
pub trait Component: Sized + 'static {
    fn name() -> &'static str { .. }                                   // default: type_name
    fn on_drop(&mut self, id: EntityId, encoder: LocalActionEncoder) {} // fires on drop/despawn
    fn on_replace(&mut self, value: &Self, id: EntityId, encoder: LocalActionEncoder) -> bool { true } // true => also fires on_drop for old value
    fn borrows() -> Vec<ComponentBorrow> { .. }                         // default: itself only
}
```
Usually via `#[derive(Component)]` (re-exported `edict_proc::Component`).
Types that don't implement `Component` can still be used as "external"
components but need manual registration (`World::ensure_external_registered`)
and **panic on use if unregistered**.

- `ComponentInfo` (`:422`) — type-erased runtime descriptor (id, layout, name,
  drop/set glue, borrow list); `ComponentInfo::of::<T: Component>()` (`:458`)
  / `::external::<T>()` (`:478`).
- `ComponentInfoRef` (`:676`) — builder from registration APIs to override
  hooks/name before they take effect: `.on_drop(hook)`/`.on_drop_fn(closure)`,
  `.on_replace(hook)`/`.on_replace_fn(closure)`, `.name(&'static str)`,
  `.finish()`.
- `DropHook<T>`/`SetHook<T>` traits (`:573`, `:591`) — pluggable hook
  interfaces; blanket-implemented for matching closures.
- `ComponentBorrow` (`:27`) — describes "component can be borrowed as type
  U" (used for `BorrowAll`/`BorrowAny`/`BorrowOne` queries); built via
  `ComponentBorrow::from_borrow`/`from_borrow_mut`/`auto`.

### Bundles (`bundle.rs`) — group components for one efficient insert/spawn
Trait ladder, all `unsafe`:
- `DynamicBundle` (`:36`) — runtime-shaped set of typed values; `valid()`,
  `contains_id`, `with_ids`, `put`.
- `DynamicComponentBundle: DynamicBundle` (`:62`) — elements implement
  `Component` (enables auto-registration).
- `Bundle: DynamicBundle` (`:77`) — static (compile-time-known-shape)
  counterpart, associated-function versions of the same queries.
- `ComponentBundle: Bundle + DynamicComponentBundle` (`:97`) — both.

All four are blanket-implemented for tuples `()` through 12 elements (letters
A–L via `for_tuple!`) — **note**: the module doc comment claims 16, the macro
expansion only covers 12; trust the macro. `valid()`/`static_valid()` is
`false` if two elements share a `TypeId` (duplicate component types in one
bundle are invalid, and bundle-consuming methods panic on this).

- `EntityBuilder` (`:258`) — dynamic runtime bundle builder for when the
  component set isn't known at compile time: `.with(v)`/`.add(v)`
  (builder-style / mut-ref-style, **replaces** on duplicate type rather than
  erroring), `.get::<T>()`/`.get_mut::<T>()`, implements `DynamicBundle` +
  `DynamicComponentBundle` (not `Bundle`/`ComponentBundle` — shape is
  dynamic). `Send` but not `Sync`, not `Clone`.

## Queries (`src/query/`)

### Trait hierarchy (post-refactor — verify against `query/mod.rs` if in doubt)
```
AsQuery              — has associated Query type (mod.rs:82)
 └─ IntoQuery: AsQuery         — value -> Self::Query (mod.rs:223)
      └─ Query: IntoQuery<Query=Self> + Copy+Send+Sync+'static  (unsafe, mod.rs:101)
DefaultQuery: Query             — zero-arg construction (mod.rs:230)
AsDefaultQuery: AsQuery<Query: DefaultQuery>   — blanket (mod.rs:237)
SendQuery: Query (unsafe, mod.rs:262)          — safe off main thread
ImmutableQuery: Query (unsafe, mod.rs:292)     — never mutates; asserts !MUTABLE
BatchQuery                                      — slice-like batch fetch, blanket-derived (mod.rs:197)
```
Plus a lattice of blanket "intersection" traits (`SendImmutableQuery`,
`DefaultSendQuery`, `AsDefaultImmutableQuery`, etc., `mod.rs:316-404`) used
purely as generic bounds — no new methods, just conjunctions of the three
above surfaced at both the `Query` level and the `AsQuery`/`IntoQuery` level
(so callers can pass `&T` instead of `Read<T>` and still satisfy the bound).

`Query` itself (`mod.rs:101`): `Item<'a>`, `Fetch<'a>: Fetch<'a>`,
`const MUTABLE: bool`, `const FILTERS_ENTITIES: bool = false`,
`component_access(&self, &ComponentInfo) -> Result<Option<Access>, WriteAlias>`,
`visit_archetype`, `access_archetype`, `fetch(arch_idx, archetype, epoch) -> Fetch`.
`FILTERS_ENTITIES = true` means matching an archetype doesn't guarantee every
entity in it is yielded (per-item filtering happens too).

`Fetch<'a>` (unsafe trait, `fetch.rs:178`) — the per-archetype cursor:
`dangling()`, `visit_chunk`, `visit_item`, `touch_chunk`, `get_item`. Call
protocol per chunk: `visit_chunk` → (if visiting) `touch_chunk` once → per
item `visit_item` → `get_item`. `BatchFetch<'a>: Fetch<'a>` (`fetch.rs:264`)
adds `get_batch(start,end) -> Batch` for contiguous-range fetch (e.g. `&[T]`).

`QueryArg`/`DefaultQueryArg` (defined in `src/system/func/view.rs`, not
`query/`) layer mutable per-run state + `before`/`after` hooks on top of
`Query`/`DefaultQuery` — this is what lets `Modified<T>` capture "epoch as of
last run" automatically when used as a system argument.

### Read/Write component mapping

| Surface syntax | Resolves to | `Item<'a>` | Mutates? | Notes |
|---|---|---|---|---|
| `&T` | `Read<T>` (`read.rs`) | `&'a T` | no | plain shared borrow |
| `&mut T` | `Write<T>` (`write.rs`) | `&'a mut T` | yes | bumps entity+chunk epoch unconditionally on fetch |
| `Alt<T>` (`alt.rs`) | `Alt<T>` | `RefMut<'a,T>` | yes (declared Write) | epoch bump deferred until `DerefMut` actually used — avoids false-positive change detection on mere access |
| `Cpy<T>` (`copied.rs`) | `Cpy<T>` | `T` (by value) | no | for small `Copy` types, skips reference indirection |
| `Option<&T>`/`Option<&mut T>` | `OptionQuery<Read/Write<T>>` (`option.rs`) | `Option<&'a (mut)T>` | matches inner | archetype not filtered out when component missing |
| `Modified<&T>` etc. (`modified/`) | `Modified<Read/Write/Alt/Cpy<T>>` | same as inner | matches inner | epoch-filters to "changed since `after_epoch`"; also has `Modified<OptionQuery<..>>` variants |
| `WithEpoch<&T>` (`with_epoch/read.rs`) | `WithEpoch<Read<T>>` | `(&'a T, EpochId)` | no | pairs data with its last-modified epoch (Read only, no Write variant exists) |
| `EpochOf<T>` (`with_epoch/mod.rs`) | `EpochOf<T>` | `EpochId` | no | epoch only, no data |
| `BorrowAny<&T>`/`<&mut T>` (`borrow/any.rs`) | `BorrowAny<Read/Write<T>>` | `&'a (mut) T` | matches inner | first component in archetype offering a borrow to `T` |
| `BorrowAll<&T>`/`<&mut T>` (`borrow/all.rs`) | `BorrowAll<Read/Write<T>>` | iterator (`BorrowAllRead`/`Write<'a,T>`) | matches inner | every component in archetype offering a borrow to `T` |
| `BorrowOne<Read/Write<T>>` (`borrow/one.rs`) | itself | `&'a (mut) T` | matches inner | component chosen at runtime by `TypeId` (`BorrowOne::new(ty)`); no `DefaultQuery` (can't default a runtime `TypeId`) |

### Filters and combinators (`filter.rs`, `boolean.rs`, `entities.rs`)
- `With<T>` (`filter.rs:285`) — archetype must contain `T`, yields `()`.
- `Without<T> = Not<With<T>>` (`filter.rs:350`).
- `Not<T>` (`filter.rs:148`) — inverts a filter; doesn't run `T`'s access
  side effects on the non-matching path.
- `And<T>`/`Or<T>`/`Xor<T>` = `BooleanQuery<T, AndOp/OrOp/XorOp>`
  (`boolean.rs:454-460`), plus fixed-arity sugar `And2..And8`/`Or2..Or8`/
  `Xor2..Xor8` (`boolean.rs:463-523`). Each arm's item comes back as
  `Option<Item>`. `Or` = at least one arm matches, `Xor` = exactly one, `And`
  = all match (but per-item, unlike a plain tuple).
- `Entities` (`entities.rs:52`) — yields `EntityLoc<'a>` for every entity
  regardless of components; no component access declared; supports
  `reserved_entity_item` (works for not-yet-spawned reserved entities).
- Tuple queries `()`, `(A,)` .. up to crate tuple arity (`tuple.rs`) — AND
  semantics; `component_access` errors with `WriteAlias` if two elements
  conflict (e.g. both write the same component).

### Known gaps / dead code in `query/` — do not rely on these
- `AnyOf<T>` (`any_of.rs`) and `Modified<AnyOf<T>>` (`modified/any_of.rs`)
  are fully implemented but **not wired into `mod.rs`** (`mod any_of;` is
  commented out) — unreachable as public API today. Use `Or`/`Or2..Or8`
  instead.
- `one_of!` macro (`one_of.rs`) only generates the enum + its `Fetch` impl —
  the `Query`/`IntoQuery`/`DefaultQuery` impls are commented out and the
  macro isn't invoked anywhere in-tree.
- `FilteredFetch<F,Q>` is exported but its producing `FilteredQuery<F,Q>:
  Query` impl is commented out — no current way to construct one.
- `src/view/relation.rs` is not declared as a `mod` in `view/mod.rs` — dead
  file, not compiled.

## Views (`src/view/`)

Single generic impl `ViewValue<'a, Q: Query, F: Query, B: BorrowState, E>`
(`mod.rs:51`) behind four public aliases:

| Alias | Borrow check | Extensible (`.with()`/`.relates()` etc.)? | Obtained from |
|---|---|---|---|
| `ViewRef<'a,Q,F=()>` | runtime (`RuntimeBorrowState`) | yes | `World::view*` (shared `&World`) |
| `ViewMut<'a,Q,F=()>` | compile-time (`StaticallyBorrowed`) | yes | `World::view*_mut` (`&mut World`) |
| `ViewCell<'a,Q,F=()>` | runtime | no | system parameter, when this system's *own* view args may alias each other |
| `View<'a,Q,F=()>` | compile-time | no | system parameter, default |

Cross-system aliasing (system A's view vs. system B's view) is always
handled by the `Scheduler`'s dependency graph — conflicting systems get
serialized, regardless of whether their args use `View` or `ViewCell`.
`ViewCell` only matters *within* one system: use it when two of that
system's own args may alias each other, so the conflict is resolved at
runtime during that system's execution instead of panicking when the
`FunctionSystem` computes its combined component access. Plain `View`
(no runtime check) is correct — and the default — whenever a system's own
args never alias each other, even if the system as a whole conflicts with
other systems.

- Obtaining: `World::new_view()`/`new_view_mut()` (empty query, then
  `.extend(..)`/`.with::<T>()`/`.without::<T>()`/relation builders), or
  `World::view::<Q>()`/`view_mut::<Q>()`/`view_with(query)`/etc.
  (`world/view.rs`). Single-entity: `World::view_one::<Q>(entity)` /
  `view_one_with::<Q>(entity, query)` → `ViewOne<'a,Q>` (`world/get.rs`).
- Single-entity access on a view: `.get`/`.get_mut`/`.try_get`/`.expect`/
  `.map`/`.try_map` (+ `_mut` variants) (`view/index.rs`). `view[entity]`
  indexing works for `Read<T>` views only.
- Iteration: `.iter()`/`.iter_mut()` → `ViewIter`/`ViewCellIter`
  (`view/iter.rs`); batch variants `.iter_batched(_mut)(batch_size)` for
  queries implementing `BatchQuery`.
- Extending an already-built view (only on `Extensible` views —
  `ViewRef`/`ViewMut`, not system-arg views): `.extend(q)`, `.filter(f)`,
  `.with::<T>()`, `.without::<T>()`, `.modified::<T>(after_epoch)`,
  `.borrow_any::<T>()`/`.borrow_all::<T>()`/`.borrow_one::<T>(ty)`, plus the
  relation builders below (`view/extend.rs`).

## Relations (`src/relation/`)

### `Relation` trait (`relation/mod.rs:42`)
```rust
pub trait Relation: Copy + 'static {
    const EXCLUSIVE: bool = false;  // one instance per origin; re-add replaces
    const SYMMETRIC: bool = false;  // adding origin->target mirrors target->origin
    const OWNED: bool = false;      // origin despawned when its last target drops
    fn name() -> &'static str { .. }
    fn on_replace(old, new, origin, old_target, new_target, encoder) -> bool { true }
    fn on_drop(self, origin, target, encoder) {}
    fn on_origin_drop(origin, targets: &[(EntityId,Self)], encoder) {}
    fn on_target_drop(origins: &[(EntityId,Self)], target, encoder) {}
}
```
Via `#[derive(Relation)]` + `#[edict(exclusive|symmetric|owned)]` attrs.
`ExclusiveRelation` sub-trait (`:137`) required by `RelatesExclusive`
queries, auto-derived when `EXCLUSIVE = true`.

Built-in: `ChildOf` (`relation/child_of.rs:7`) — `EXCLUSIVE=true, OWNED=true`
(child despawned when parent despawned), `SYMMETRIC=false`.

### Adding/removing (`World` methods, `world/relation.rs`)
- `insert_relation::<R>(&mut self, origin, relation: R, target)` (`:26`) —
  replaces existing; handles `EXCLUSIVE`/`SYMMETRIC` automatically.
- `remove_relation::<R>(&mut self, origin, target) -> Option<R>` (`:100`) —
  **does not** fire `on_drop`.
- `drop_relation::<R>(&mut self, origin, target)` (`:118`) — fires `on_drop`.

### Querying (`relation/query/`) — naming: `*Relates*` matches origins, `*Related*` matches targets
| Query | Matches | Item |
|---|---|---|
| `Relates<With<R>>` | origins | `RelationIter<'a,R>` (all targets, entity only) |
| `Relates<&R>`/`<&mut R>` | origins | `RelationReadIter`/`WriteIter<'a,R>` (all `(target, &(mut)R)`) |
| `RelatesExclusive<With/&/&mut R>` | origins of `R: ExclusiveRelation` | single item, not an iterator (at most one target) |
| `RelatesTo<R>::new(target)` + `&R`/`&mut R` | origins relating to one specific target | `&'a (mut) R`; filters entities |
| `Related<With/&/&mut R>` | targets | mirrors `Relates` shape, handles `SYMMETRIC` internally |
| `FilterRelated<R>` / `FilterRelates<R>` | filter only, `Item=()` | is-a-target-of-R / is-an-origin-of-R |
| `FilterRelatedBy<R>::new(origin)` / `FilterRelatesTo<R>::new(target)` | filter only | target-of-R-from-this-origin / origin-of-R-to-this-target |

No `FilterNotRelates`/etc. types exist — use `Not<FilterRelates<R>>` etc.
View builder sugar for all of the above: `.relates::<R>()`,
`.relates_ref/_mut::<R>()`, `.relates_to(_mut)::<R>(target)`,
`.relates_exclusive(_ref/_mut)::<R>()`, `.related(_ref/_mut)::<R>()`,
`.filter_related(_by)::<R>(..)`, `.filter_relates(_to)::<R>(..)`
(`view/extend.rs:238-436`, extensible views only).

## Systems (`src/system/`)

### Writing a system
Any `fn`/closure whose every parameter implements `FnArg` gets `IntoSystem`
via a blanket impl (`func/mod.rs:230`) — that's the whole mechanism. Add
`#[edict::system]` above it purely for better compile errors (it's a
compile-time-only validation macro, no runtime effect); rejects `async`,
`unsafe`, generics, non-`()` return, `self` receiver.

### `FnArg` implementors and what they cost the scheduler

| Arg type | Local (main-thread only)? | World/component access |
|---|---|---|
| `ActionEncoder<'_>` | no | none declared |
| `Res<'a,T: Sync>` | no | Read of resource `T` |
| `ResMut<'a,T: Send>` | no | Write of resource `T` |
| `ResLocal<'a,T>` | **yes** | for `!Sync` resources |
| `ResMutLocal<'a,T>` | **yes** | for `!Send` resources |
| `State<'a,T: Default+Send>` | no | none — value lives in the system's own persistent state, not in `World` |
| `View<'a,Q,F>` | no | per query; conflicts with *this system's own* other args panic when the system is built |
| `ViewCell<'a,Q,F>` | no | per query; conflicts with *this system's own* other args resolved at runtime instead of panicking |
| `&World` | no | conservatively conflicts with everything |
| `&mut World` | **yes** | conservatively conflicts with everything |

Prefer `Res`/`ResMut`/`View` over the `Local`/`&(mut) World` variants — the
latter force sequential scheduling. Note `View` vs. `ViewCell` only governs
conflicts *within one system's own argument list* — conflicts *between*
different systems are always resolved by the `Scheduler` serializing them
(registration-order dependency), never by this choice.

### Running systems (`src/scheduler/`)
`Scheduler::new()` → `.add_system(fn_or_closure_or_system)` per system →
`.run_sequential(&mut world)` (no parallelism) or `.run_threaded(&mut world)`
(feature `threaded-scheduler`) or `.run_rayon(&mut world)` (feature
`rayon-scheduler`) or `.run_with(&mut world, &executor)` (custom
`ScopedExecutor` impl, e.g. for debugging on the calling thread only).
Conflict resolution is **registration order**: later-added reader sees
earlier-added writer's effects; earlier-added reader does not see
later-added writer's; scheduling is recomputed only when the world's
archetype set changes (cached by `archetype_set_id()`).

## Actions (`src/action/`) — deferred `World` mutation

Two parallel families: **non-local** (`ActionEncoder`/`ActionBuffer`/
`SpawnBatch`, `Send`-bounded, usable with the threaded scheduler) and
**local** (`LocalActionEncoder`/`LocalActionBuffer`/`LocalSpawnBatch`,
`!Send`-permitted, single-thread only, used by hooks). Method surface is
structurally identical between the two.

`ActionEncoder` methods (`action/encoder.rs`) mostly defer — nothing runs
until the buffer is executed — with two exceptions that act immediately:
`allocate()` (reserves an `EntityId` synchronously via `EntitySet::alloc`,
same mechanism as `World::allocate`) and `is_alive` (checks liveness right
away). `spawn_one`/`spawn`/`spawn_external` call `allocate()` internally, so
**the id comes back immediately and is valid right away** — only the actual
component insertion (materializing the entity into an archetype) is
deferred until the buffer runs. Everything else genuinely defers:
`spawn_batch`/`spawn_external_batch` (→ `SpawnBatch`), `despawn`/
`despawn_batch`, `insert`/`insert_external`, `with`/`with_external` (lazy,
`FnOnce() -> T`), `insert_bundle`/`insert_external_bundle`, `with_bundle`/
`with_external_bundle`, `drop`/`drop_batch`, `drop_erased(_batch)`,
`drop_bundle`, `insert_relation`/`drop_relation`, `insert_resource`/
`drop_resource`, `closure(fn)` (escape hatch for arbitrary `World`
mutation). `reborrow()` just hands out a shorter-lived handle to the same
buffer — not an action itself. Every deferred method is sugar over pushing a
closure that calls the equivalent `World` method.

`ActionSender` (below) is the one place that genuinely **can't** allocate an
id up front — it has no `allocate()`/`EntityLoc`-returning methods at all,
since it only pushes into a lock-free queue with no synchronous return path.

Outside the scheduler (another thread / async task without an
`ActionEncoder`): `World::new_action_sender() -> ActionSender` (`Clone`,
mirrors most `ActionEncoder` methods but acts immediately via a lock-free
queue, no `EntityId`-returning variants since it can't allocate); check
`.is_connected()`; apply queued actions with
`World::execute_received_actions()`.

## Flows (`src/flow/`) — async tasks with implicit `World` access

- `Flows` (`flow/mod.rs:324`) — container that owns spawned flows; pair one
  per `World`. `Flows::execute(&mut self, world)` runs maintenance, drains
  newly-spawned flows, polls ready ones. Ready-made system:
  `flows_system(world, flows: State<Flows>)` — just add it to your schedule.
- Spawn: `World::spawn_flow(flow)` / `spawn_flow_for(entity, flow)`, or from
  inside a running flow, `FlowWorld::spawn_flow(self, flow)` /
  `FlowEntity::spawn_flow(self, flow)`. A flow value is any
  `async fn(world: FlowWorld)` (world-scoped, `IntoFlow`) or
  `async fn(e: FlowEntity)` (entity-scoped, `IntoEntityFlow`; auto-cancelled
  if the entity despawns).
- `FlowWorld` (`flow/world.rs`) — zero-sized `Copy` handle, mirrors most of
  `World`'s mutating API 1:1 (`insert`, `spawn*`, `despawn`, resources,
  relations — all by value `self`, safe across `.await` points) plus
  `.poll(f)`/`.poll_view::<Q>(f)`/`.poll_view_filter::<Q,F>(f)` for
  low-level custom awaitables and `.entity(id) -> Result<FlowEntity,_>`.
- `FlowEntity` (`flow/entity.rs`) — entity-scoped handle; same shape, plus
  `try_*` variants returning `Result` instead of panicking on despawn, and
  `.poll_view`/`.try_poll_view` for awaiting a query match on this entity.
- Helpers (`flow/futures.rs`): `yield_now!()` macro — yield one executor
  turn; `WakeOnDrop` component — attached ad hoc to entities flows are
  waiting on, wakes registered wakers on despawn;
  `FlowEntity::wait_despawned()` / `wait_has_component::<T>()` — ready-made
  awaitables built on the above.
- Requires the `"flow"` feature (default-on).

## World / Resources / Epoch / Archetype

### `World` (`src/world/mod.rs:139`) — `Sync`, explicitly **not `Send`**
Core surface is huge; grouped by file:
- **Get** (`world/get.rs`): `get::<Q>(entity)`/`get_with`, `_unchecked`
  variants, `view_one::<Q>`/`view_one_with`, `get_cloned::<T: Clone>`.
- **Insert** (`world/insert.rs`): `insert::<T: Component>` (replaces),
  `insert_external::<T>` (replaces — **doc comment in the source says
  "preserved", that's wrong, trust the replace behavior**), `with::<T>`
  (inserts only if missing — **doc comment says "replaced", also wrong**;
  preserves existing), `insert_bundle`/`insert_external_bundle`,
  `with_bundle`/`with_external_bundle` (missing-only, no hooks fire, entity
  cannot be despawned by the call). `WorldLocal` adds `*_defer` variants of
  all of these (queued, silently drop `NoSuchEntity`).
- **Remove** (`world/remove.rs`): `remove::<T>`, `drop::<T>`,
  `drop_batch::<T>` (missing entities silently skipped), `drop_erased(_batch)`
  (by `TypeId`), `drop_bundle::<B: Bundle>` (doesn't require bundle
  components to be pre-registered).
- **Relation** (`world/relation.rs`): see Relations section above.
- **Resource** (`world/resource.rs`): `insert_resource`/`with_resource`/
  `with_default_resource`/`remove_resource`, `get_resource::<T: Sync>`/
  `expect_resource` (panics)/`copy_resource`/`clone_resource` (panic if
  missing), `get_resource_mut::<T: Send>`/`expect_resource_mut`,
  `get_local_resource(_mut)::<T>` (unsafe, bypasses Sync/Send — prefer
  `.local()` wrapper or the Sync/Send-bounded versions), `undo_resource_leaks`
  (needed after `Res::leak`/`ResMut::leak`).
- **Spawn** (`world/spawn.rs`): `allocate()` (reserve id, lazy materialize;
  panics if id space exhausted), `spawn_empty`/`spawn_one`/`spawn`/
  `spawn_at` (panics if id in use)/`spawn_or_insert` (inserts into existing
  if id in use)/`spawn_external(_at)`, `spawn_batch`/`spawn_batch_external`
  → `SpawnBatch` iterator (`.spawn_all()` drains without collecting ids),
  `despawn`/`despawn_batch`. `WorldLocal` adds `*_defer` variants — entities
  from these are alive immediately (reserved) but components only appear
  after `run_deferred()`.
- **View** (`world/view.rs`): see Views section above.
- **Builder** (`world/builder.rs`): `WorldBuilder::new()` →
  `.register_component::<T>()`/`.register_external::<T>()` (customize hooks
  pre-`World`) → `.with_id_range_allocator(alloc)` → `.build()`.
  `World::ensure_component_registered`/`ensure_external_registered`
  (**required** before `insert_external*`/`spawn_external*` — no implicit
  registration for non-`Component` types)/`ensure_bundle_registered`/
  `assert_bundle_registered` (panics if any type unregistered).
- `WorldLocal` (`world/mod.rs:412`) — `#[repr(transparent)]` wrapper proven
  thread-confined; `.defer(closure)` queues a mutation, applied by
  `run_deferred()` or automatically before the next mutating op. Get one via
  `World::local()`.
- Misc: `archetype_set_id()` (changes when archetypes added/removed —
  scheduler cache key), `epoch()`, `has_component::<T>`, `is_alive`.

### `Resources` (`resources.rs`) — `Res<T>`/`ResMut<T>` guards
`Deref`/`DerefMut` wrappers with `Res::leak(r) -> &'a T` / `ResMut::leak(r)
-> &'a mut T` (associated fns, not methods) to intentionally hold a borrow
past the guard's lifetime — must call `World::undo_resource_leaks()` before
a conflicting borrow, or it panics.

### `EpochCounter`/`EpochId` (`epoch.rs`) — change-detection clock
Monotonic `AtomicU64`. `EpochCounter::next()`/`next_mut()`/`current()`.
`EpochId::before`/`after`/`update` (bump to max), `bump` (asserts strictly
forward), `bump_again` (asserts not-after — allows same-epoch idempotent
touches, e.g. batch ops touching the same entity twice in one pass).

### `Archetype` (`archetype.rs:185`)
Exposed specifically so **custom `Query` implementations** can interact with
it — most methods are `unsafe` and not meant for ordinary application code.
Key ones if you're writing a custom `Query`/`Fetch`: `has_component`,
`matches`, `ids()`/`infos()`, `get::<T>`/`get_mut::<T>` (bumps epoch)/
`get_mut_nobump::<T>` (does not touch change-detection — for internal
mutation that must stay invisible to `Modified<T>` queries), `spawn*`,
`despawn(_unchecked)`, `insert_bundle`/`remove`/`drop_bundle` (archetype
transition operations, all `unsafe`, safety = destination archetype's
component set must match exactly).

## World serialization (`src/dump/`) — secondary/opt-in feature

Purpose: dump a chosen tuple of component types out of `World` (optionally
delta — only components changed after an `EpochId`) and load it back via
`ActionEncoder`-recorded commands. Core types: `DumpSet`/`LoadSet` traits
(implemented for tuples), `WorldDump<'a,T,F>`/`WorldLoad<'a,T,M>` wrappers,
`Dumper<T>`/`Loader<T>` traits backends implement. Three feature-gated
backends, each adding serialize/deserialize impls on `WorldDump`/`WorldLoad`:
`dump/serde.rs` (`feature = "serde"`, dump-only, no load side in this file),
`dump/nanoserde.rs` (`feature = "nanoserde"`, `SerBin`/`DeBin`, adds
`WorldLoad::load_bin`), `dump/alkahest.rs` (`feature = "alkahest"`, zero-copy
formulas, `WorldDump::dump_alkahest`/`WorldLoad::load_alkahest(_lazy)`). Not
part of the core ECS hot path — treat as opt-in, only dig in when a task
specifically touches serialization.
