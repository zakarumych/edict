# Located access: assembly evidence

Inspection date: 2026-10-22. Stable rustc 1.99.1 release build; `phases` disabled. The diagnostic variants share `Global`, `LocalPosition`, and `Marks` from `src/edict_bevy_backend.rs:17`; `src/edict_located_backend.rs:15` imports those definitions. Thus their component TypeIds are identical within this build.

Reproduce from `C:/code/edict`:

```powershell
cargo +stable rustc --release --offline --lib --manifest-path experiments/aethos-transform-diagnosis/Cargo.toml -- --emit=asm
```

Inspected artifact: `target/release/deps/aethos_transform_diagnosis-a5386ab4165f67ce.s`, SHA256 `837BA3073471641126400418712B00F16F7DDEEB764A7B8C2965E119DCBD9CD4`. Assembly line numbers below refer to that artifact and can move on rebuild.

## Confirmed code generation

- Baseline hierarchy `try_get<EntityId>` at assembly line 48505 contains the component-fetch logic inline. Its only calls are panic paths.
- Located hierarchy `try_get<EntityId>` at line 49280 calls separate `get_at` at line 88444. It then copies the 48-byte result from its stack into the caller's result slot using three `movups` loads and three `movups` stores. This adds a call/return and tuple copy on each successful recursive update.
- Baseline recursive `update` at line 167758 allocates 152 stack bytes; located `update` at line 171627 allocates 184. Located update copies the 16-byte `EntityLoc` to a stack argument and passes its address to an out-of-line global getter. Baseline passes its `EntityId` in a register.
- Baseline recursive `cover` starts at line 167688; located `cover` at line 171544. Located cover invokes `get_at` directly after its marker lookup. It does not incur the extra hierarchy wrapper call seen in located update.
- Located `get_at` still contains separate presence and fetch probes for optional components. Location reuse does not cache component-column resolution.

## Source explanation and limits

`src/entity/mod.rs:379` in the edict repository implements `EntityLoc::lookup` by returning its stored location; the entity-map lookup is genuinely skipped. `src/query/entities.rs:33` obtains an entity ID from the archetype's entity column to construct the returned location. Adding `Entities` therefore changes both the returned data and its calling convention.

The access pipeline is in edict `src/view/index.rs:42`, `src/view/index.rs:177`, and `src/view/mod.rs:471`. The diagnostic recursion is in `src/edict_located_backend.rs:33` and `src/edict_located_backend.rs:64`.

The shared-component timing result can consequently coexist with the scalar microbenchmark's benefit from supplying an already available location: the transform variant removes an entity lookup while adding real data movement and an extra call. Assembly establishes these mechanisms, not their individual elapsed-time shares. It does not establish that the extra call alone accounts for the entire located regression.
