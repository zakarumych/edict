use std::collections::VecDeque;

use edict::{
    EntityError,
    action::ActionBuffer,
    component::Component,
    dump::{DumpSet, DumpSlot, Dumper, EntityDump, LoadSet, LoadSlot, Loader, NoMark},
    entity::EntityId,
    epoch::EpochId,
    query::{Cpy, With},
    world::World,
};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct A(i32);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct B(i32);
#[derive(Component)]
struct Selected;

#[derive(Clone, Debug)]
struct Record {
    header: [u64; 3],
    a: Option<A>,
    b: Option<B>,
}

#[derive(Default)]
struct MemoryDump(Vec<Record>);

impl Dumper<(A, B)> for MemoryDump {
    type Error = &'static str;

    fn dump(
        &mut self,
        entity: EntityDump,
        (a, b): (DumpSlot<'_, A>, DumpSlot<'_, B>),
    ) -> Result<(), Self::Error> {
        self.0.push(Record {
            header: entity.0,
            a: match a {
                DumpSlot::Skipped => None,
                DumpSlot::Component(a) => Some(*a),
            },
            b: match b {
                DumpSlot::Skipped => None,
                DumpSlot::Component(b) => Some(*b),
            },
        });
        Ok(())
    }
}

struct MemoryLoad {
    records: VecDeque<Record>,
    current: Option<Record>,
}

impl Loader<(A, B)> for MemoryLoad {
    type Error = &'static str;

    fn next(&mut self) -> Result<Option<EntityDump>, Self::Error> {
        self.current = self.records.pop_front();
        Ok(self.current.as_ref().map(|r| EntityDump(r.header)))
    }

    fn load(&mut self, (a, b): &mut (LoadSlot<'_, A>, LoadSlot<'_, B>)) -> Result<(), Self::Error> {
        let record = self.current.as_ref().unwrap();
        match a {
            LoadSlot::Skipped => {}
            LoadSlot::Missing => *a = LoadSlot::Created(record.a.ok_or("missing A payload")?),
            LoadSlot::Existing(a) => **a = record.a.ok_or("missing A payload")?,
            LoadSlot::Created(_) => unreachable!(),
        }
        match b {
            LoadSlot::Skipped => {}
            LoadSlot::Missing => *b = LoadSlot::Created(record.b.ok_or("missing B payload")?),
            LoadSlot::Existing(b) => **b = record.b.ok_or("missing B payload")?,
            LoadSlot::Created(_) => unreachable!(),
        }
        Ok(())
    }
}

fn dump(world: &World, epoch: EpochId) -> Vec<Record> {
    let mut output = MemoryDump::default();
    <(A, B)>::dump_world(world, (), epoch, &mut output).unwrap();
    output.0
}

fn load(world: &mut World, records: Vec<Record>) -> Result<(), &'static str> {
    let mut actions = ActionBuffer::new();
    let mut loader = MemoryLoad {
        records: records.into(),
        current: None,
    };
    <(A, B)>::load_world(world, NoMark, &mut actions.encoder(world), &mut loader)?;
    actions.execute(world);
    Ok(())
}

fn record(id: EntityId, present: u64, modified: u64, a: Option<A>, b: Option<B>) -> Record {
    Record {
        header: [id.bits(), present, modified],
        a,
        b,
    }
}

#[test]
fn empty_world_dump_and_empty_load_are_noops() {
    let mut world = World::new();
    assert!(dump(&world, EpochId::start()).is_empty());
    load(&mut world, vec![]).unwrap();
    assert_eq!(world.new_view().into_iter().count(), 0);
}

#[test]
fn full_dump_records_component_presence_and_payloads() {
    let mut world = World::new();
    let a = world.spawn_one(A(10)).id();
    let both = world.spawn((A(20), B(30))).id();
    let rows = dump(&world, EpochId::start());
    let a_row = rows.iter().find(|r| r.header[0] == a.bits()).unwrap();
    assert_eq!(a_row.header, [a.bits(), 1, 1]);
    assert_eq!((a_row.a, a_row.b), (Some(A(10)), None));
    let both_row = rows.iter().find(|r| r.header[0] == both.bits()).unwrap();
    assert_eq!(both_row.header, [both.bits(), 3, 3]);
    assert_eq!((both_row.a, both_row.b), (Some(A(20)), Some(B(30))));
}

#[test]
fn dump_filter_limits_entities_without_serializing_filter_component() {
    let mut world = World::new();
    world.spawn_one(A(10));
    let selected = world.spawn((A(20), B(30), Selected)).id();
    let mut output = MemoryDump::default();
    <(A, B)>::dump_world(&world, With::<Selected>, EpochId::start(), &mut output).unwrap();
    assert_eq!(output.0.len(), 1);
    assert_eq!(output.0[0].header, [selected.bits(), 3, 3]);
}

#[test]
fn delta_dump_keeps_presence_but_skips_unchanged_payload() {
    let mut world = World::new();
    let id = world.spawn((A(1), B(2))).id();
    let epoch = world.epoch();
    world.insert(id, B(3)).unwrap();
    let rows = dump(&world, epoch);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].header, [id.bits(), 3, 2]);
    assert_eq!((rows[0].a, rows[0].b), (None, Some(B(3))));
}

#[test]
fn full_roundtrip_restores_ids_and_values() {
    let mut source = World::new();
    let first = source.spawn((A(1), B(2))).id();
    let second = source.spawn_one(B(3)).id();
    let mut target = World::new();
    load(&mut target, dump(&source, EpochId::start())).unwrap();
    assert_eq!(target.get::<(Cpy<A>, Cpy<B>)>(first).unwrap(), (A(1), B(2)));
    assert_eq!(target.get::<Cpy<B>>(second).unwrap(), B(3));
    assert_eq!(target.get::<&A>(second), Err(EntityError::Mismatch));
}

#[test]
fn loading_existing_component_updates_and_preserves_it() {
    let mut world = World::new();
    let id = world.spawn_one(A(1)).id();
    load(&mut world, vec![record(id, 1, 1, Some(A(9)), None)]).unwrap();
    assert_eq!(world.get::<Cpy<A>>(id), Ok(A(9)));
}

#[test]
fn delta_load_preserves_present_unmodified_component() {
    let mut world = World::new();
    let id = world.spawn((A(1), B(2))).id();
    load(&mut world, vec![record(id, 3, 2, None, Some(B(9)))]).unwrap();
    assert_eq!(world.get::<Cpy<A>>(id), Ok(A(1)));
    assert_eq!(world.get::<Cpy<B>>(id), Ok(B(9)));
}

#[test]
fn load_removes_components_absent_from_serialized_presence() {
    let mut world = World::new();
    let id = world.spawn((A(1), B(2))).id();
    load(&mut world, vec![record(id, 0, 0, None, None)]).unwrap();
    assert!(world.is_alive(id));
    assert_eq!(world.get::<&A>(id), Err(EntityError::Mismatch));
    assert_eq!(world.get::<&B>(id), Err(EntityError::Mismatch));
}

#[test]
fn truncated_component_payload_propagates_loader_error() {
    let mut world = World::new();
    let id = world.spawn_empty().id();
    assert_eq!(
        load(&mut world, vec![record(id, 1, 1, None, None)]),
        Err("missing A payload")
    );
    assert_eq!(world.get::<&A>(id), Err(EntityError::Mismatch));
}

#[test]
fn dumper_error_propagates_without_touching_world() {
    struct Reject;
    impl Dumper<(A, B)> for Reject {
        type Error = &'static str;
        fn dump(
            &mut self,
            _: EntityDump,
            _: (DumpSlot<'_, A>, DumpSlot<'_, B>),
        ) -> Result<(), Self::Error> {
            Err("output full")
        }
    }
    let mut world = World::new();
    let id = world.spawn_one(A(1)).id();
    let epoch = world.epoch();
    assert_eq!(
        <(A, B)>::dump_world(&world, (), EpochId::start(), &mut Reject),
        Err("output full")
    );
    assert_eq!(world.epoch(), epoch);
    assert_eq!(world.get::<Cpy<A>>(id), Ok(A(1)));
}
