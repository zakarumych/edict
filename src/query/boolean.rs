use core::{any::TypeId, marker::PhantomData, ops::ControlFlow};

use crate::{
    archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId,
    system::QueryArg,
};

use super::{
    Access, AsQuery, DefaultQuery, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias,
    fetch::{BatchFetch, Fetch},
};

/// Binary operator for [`BooleanQuery`].
pub trait BooleanFetchOp: 'static {
    /// Applies binary operator to two values.
    /// Returns `ControlFlow::Continue` if the operation should continue.
    /// Returns `ControlFlow::Break` if applying anything else would not change the result.
    fn op(a: bool, b: bool) -> ControlFlow<bool, bool>;

    /// Returns `true` if the result of the operation is `true` for the given mask.
    /// The mask is a bitset where each bit represents a single value.
    /// `count` is the number of bits to consider in the mask.
    fn mask(mask: u16, count: usize) -> bool;
}

pub enum AndOp {}

impl BooleanFetchOp for AndOp {
    #[inline]
    fn op(a: bool, b: bool) -> ControlFlow<bool, bool> {
        if a && b {
            ControlFlow::Continue(true)
        } else {
            ControlFlow::Break(false)
        }
    }

    #[inline]
    fn mask(mask: u16, count: usize) -> bool {
        debug_assert_eq!(mask & !((1 << count) - 1), 0);
        mask == (1 << count) - 1
    }
}

pub enum OrOp {}

impl BooleanFetchOp for OrOp {
    #[inline]
    fn op(a: bool, b: bool) -> ControlFlow<bool, bool> {
        if a || b {
            ControlFlow::Break(true)
        } else {
            ControlFlow::Continue(false)
        }
    }

    #[inline]
    fn mask(mask: u16, count: usize) -> bool {
        debug_assert_eq!(mask & !((1 << count) - 1), 0);
        mask != 0
    }
}

pub enum XorOp {}

impl BooleanFetchOp for XorOp {
    #[inline]
    fn op(a: bool, b: bool) -> ControlFlow<bool, bool> {
        match (a, b) {
            (false, false) => ControlFlow::Continue(false),
            (true, true) => ControlFlow::Break(false),
            _ => ControlFlow::Continue(true),
        }
    }

    #[inline]
    fn mask(mask: u16, count: usize) -> bool {
        debug_assert_eq!(mask & !((1 << count) - 1), 0);
        mask.is_power_of_two()
    }
}

/// Combines multiple queries.
/// Applies boolean operation to query filtering.
/// Yields tuple of query items wrapper in `Option`.
pub struct BooleanQuery<T, Op> {
    tuple: T,
    op: PhantomData<fn() -> Op>,
}

impl<T, Op> Default for BooleanQuery<T, Op>
where
    T: Default,
{
    #[inline]
    fn default() -> Self {
        BooleanQuery {
            tuple: Default::default(),
            op: PhantomData,
        }
    }
}

impl<T, Op> Clone for BooleanQuery<T, Op>
where
    T: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        BooleanQuery {
            tuple: self.tuple.clone(),
            op: PhantomData,
        }
    }
}

impl<T, Op> Copy for BooleanQuery<T, Op> where T: Copy {}

impl<T, Op> BooleanQuery<T, Op> {
    /// Creates a new [`BooleanQuery`].
    #[inline]
    pub fn from_tuple(tuple: T) -> Self {
        BooleanQuery {
            tuple,
            op: PhantomData,
        }
    }
}

/// Boolean filter combines two filters and boolean operation.
pub struct BooleanFetch<T, Op> {
    tuple: T,
    archetype: u16,
    chunk: u16,
    item: u16,
    filters_entities: bool,
    op: PhantomData<Op>,
}

fn is_xor<B: BooleanFetchOp>() -> bool {
    crate::type_id::<XorOp>() == crate::type_id::<B>()
}

macro_rules! impl_boolean {
    () => { /* Don't implement for empty tuple */ };
    ($($a:ident)+) => {
        #[allow(non_snake_case)]
        #[allow(unused_variables, unused_mut, unused_assignments)]
        unsafe impl<'a, Op $(, $a)+> Fetch<'a> for BooleanFetch<($($a,)+), Op>
        where
            $($a: Fetch<'a>,)+
            Op: BooleanFetchOp,
        {
            type Item = ($(Option<$a::Item>,)+);

            #[inline]
            fn dangling() -> Self {
                BooleanFetch {
                    tuple: ($($a::dangling(),)+),
                    archetype: 0,
                    chunk: 0,
                    item: 0,
                    filters_entities: false,
                    op: PhantomData,
                }
            }

            #[inline]
            unsafe fn get_item(&mut self, idx: u32) -> ($(Option<$a::Item>,)+) {
                let ($($a,)+) = &mut self.tuple;
                let mut mi = 1;
                ($({
                    let elem = if self.item & mi != 0 {
                        Some(unsafe { $a.get_item(idx) })
                    } else {
                        None
                    };
                    mi <<= 1;
                    elem
                },)+)
            }

            #[inline]
            unsafe fn visit_item(&mut self, idx: u32) -> bool {
                self.item = 0;
                let ($($a,)+) = &mut self.tuple;
                let mut mi = 1;
                let mut count = 0;
                $(
                    if self.chunk & mi != 0 {
                        if unsafe { $a.visit_item(idx) } {
                            self.item |= mi;
                        }
                    }
                    mi <<= 1;
                    count += 1;
                )+
                Op::mask(self.item, count)
            }

            #[inline]
            unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
                self.chunk = 0;
                let ($($a,)+) = &mut self.tuple;
                let mut mi = 1;
                let mut count = 0;
                $(
                    if self.archetype & mi != 0 {
                        if unsafe { $a.visit_chunk(chunk_idx) } {
                            self.chunk |= mi;
                        }
                    }
                    mi <<= 1;
                    count += 1;
                )+


                if self.filters_entities && is_xor::<Op>() && self.chunk != 0 {
                    true
                } else {
                    Op::mask(self.chunk, count)
                }
            }

            #[inline]
            unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
                let ($($a,)+) = &mut self.tuple;
                let mut mi = 1;
                $(
                    if self.chunk & mi != 0 {
                        unsafe { $a.touch_chunk(chunk_idx) };
                    }
                    mi <<= 1;
                )+
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_variables, unused_mut, unused_assignments)]
        unsafe impl<'a, Op $(, $a)+> BatchFetch<'a> for BooleanFetch<($($a,)+), Op>
        where
            $($a: BatchFetch<'a>,)+
            Op: BooleanFetchOp,
        {
            type Batch = ($(Option<$a::Batch>,)+);

            #[inline]
            unsafe fn get_batch(&mut self, start: u32, end: u32) -> ($(Option<$a::Batch>,)+) {
                let ($($a,)+) = &mut self.tuple;
                let mut mi = 1;
                ($({
                    let elem = if self.archetype & mi != 0 {
                        Some(unsafe { $a.get_batch(start, end) })
                    } else {
                        None
                    };
                    mi <<= 1;
                    elem
                },)+)
            }
        }

        #[allow(non_snake_case)]
        impl<'a, Op $(, $a)+> BooleanQuery<($($a,)+), Op>
        where
            Op: BooleanFetchOp,
        {
            /// Creates a new [`BooleanQuery`].
            #[inline]
            #[allow(clippy::too_many_arguments)]
            pub fn new($($a: $a),+) -> Self {
                BooleanQuery {
                    tuple: ($($a,)+),
                    op: PhantomData
                }
            }
        }

        #[allow(non_snake_case)]
        impl<Op $(, $a)+> AsQuery for BooleanQuery<($($a,)+), Op>
        where
            $($a: AsQuery,)+
            Op: BooleanFetchOp,
        {
            type Query = BooleanQuery<($($a::Query,)+), Op>;
        }

        #[allow(non_snake_case)]
        impl<Op $(, $a)+> IntoQuery for BooleanQuery<($($a,)+), Op>
        where
            $($a: IntoQuery,)+
            Op: BooleanFetchOp,
        {
            #[inline]
            fn into_query(self) -> Self::Query {
                let ($($a,)+) = self.tuple;
                BooleanQuery {
                    tuple: ($($a.into_query(),)+),
                    op: PhantomData,
                }
            }
        }

        #[allow(non_snake_case)]
        impl<Op $(, $a)+> DefaultQuery for BooleanQuery<($($a,)+), Op>
        where
            $($a: DefaultQuery,)+
            Op: BooleanFetchOp,
        {
            #[inline]
            fn default_query() -> Self::Query {
                BooleanQuery {
                    tuple: ($(<$a as DefaultQuery>::default_query(),)+),
                    op: PhantomData,
                }
            }
        }

        impl<Op $(, $a)+> QueryArg for BooleanQuery<($($a,)+), Op>
        where
            $($a: QueryArg,)+
            Op: BooleanFetchOp,
        {
            #[inline]
            fn new() -> Self {
                BooleanQuery {
                    tuple: ($(<$a as QueryArg>::new(),)+),
                    op: PhantomData,
                }
            }
            #[inline]
            fn before(&mut self, world: &crate::world::World) {
                self.tuple.before(world);
            }

            #[inline]
            fn after(&mut self, world: &crate::world::World) {
                self.tuple.after(world);
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_variables, unused_mut, unused_assignments)]
        unsafe impl<Op $(, $a)+> Query for BooleanQuery<($($a,)+), Op>
        where
            $($a: Query,)+
            Op: BooleanFetchOp,
        {
            type Item<'a> = ($(Option<$a::Item<'a>>,)+) where $($a: 'a,)+;
            type Fetch<'a> = BooleanFetch<($($a::Fetch<'a>,)+), Op> where $($a: 'a,)+;

            const MUTABLE: bool = $($a::MUTABLE ||)+ false;
            const FILTERS_ENTITIES: bool = $($a::FILTERS_ENTITIES ||)+ false;

            #[inline]
            fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
                let mut unknown = false;
                if Self::FILTERS_ENTITIES && is_xor::<Op>() {
                    let mut count = 0;
                    $(
                        match <$a as Query>::cache_visit_archetype(archetype) {
                            // Exactly-one matching remains possible when extra arms
                            // can be removed by entity filtering.
                            Some(true) => return Some(true),
                            Some(false) => {}
                            None => unknown = true,
                        }
                        count += 1;
                    )+
                    if unknown { None } else { Some(Op::mask(0, count)) }
                } else {
                    let mut result = Op::mask(0, 0);
                    $(
                        match <$a as Query>::cache_visit_archetype(archetype) {
                            Some(value) => match Op::op(result, value) {
                                ControlFlow::Break(result) => return Some(result),
                                ControlFlow::Continue(value) => result = value,
                            },
                            None => unknown = true,
                        }
                    )+
                    if unknown { None } else { Some(result) }
                }
            }

            #[inline]
            fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
                let ($($a,)+) = &self.tuple;
                let mut result = None;
                $(
                    result = match (result, $a.component_access(comp)?) {
                        (None, one) | (one, None) => one,
                        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
                        _ => return Err(WriteAlias),
                    };
                )*
                Ok(result)
            }

            #[inline]
            unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
                let ($($a,)+) = &self.tuple;
                $(if $a.visit_archetype(archetype) {
                    unsafe { $a.access_archetype(archetype, &mut f); }
                })+
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = &self.tuple;
                let mut mi = 1;
                let mut count = 0;
                let mut mask = 0;
                $(
                    if $a.visit_archetype(archetype) {
                        mask |= mi;
                    }
                    mi <<= 1;
                    count += 1;
                )+

                if Self::FILTERS_ENTITIES && mask != 0 && is_xor::<Op>() {
                    true
                } else {
                    Op::mask(mask, count)
                }
            }

            #[inline]
            unsafe fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = &self.tuple;
                let mut mi = 1;
                let mut count = 0;
                let mut mask = 0;
                $(
                    if $a.visit_archetype(archetype)
                        && unsafe { $a.visit_archetype_late(archetype) }
                    {
                        mask |= mi;
                    }
                    mi <<= 1;
                    count += 1;
                )+

                if Self::FILTERS_ENTITIES && is_xor::<Op>() && mask != 0 {
                    true
                } else {
                    Op::mask(mask, count)
                }
            }

            #[inline]
            unsafe fn fetch<'a>(
                &self,
                arch_idx: u32,
                archetype: &'a Archetype,
                epoch: EpochId,
            ) -> BooleanFetch<($($a::Fetch<'a>,)+), Op> {
                let ($($a,)+) = &self.tuple;
                let mut mask = 0;
                let mut mi = 0;

                $(
                    let $a = if $a.visit_archetype(archetype)
                        && unsafe { $a.visit_archetype_late(archetype) }
                    {
                        mask |= (1 << mi);
                        unsafe { $a.fetch(arch_idx, archetype, epoch) }
                    } else {
                        Fetch::dangling()
                    };
                    mi += 1;
                )+

                BooleanFetch {
                    tuple: ($($a,)+),
                    archetype: mask,
                    chunk: 0,
                    item: 0,
                    filters_entities: Self::FILTERS_ENTITIES,
                    op: PhantomData,
                }
            }

            #[inline]
            fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<Self::Item<'a>> {
                let ($($a,)+) = &self.tuple;
                let mut mask = 0;
                let mut mi = 0;
                $(
                    let $a = $a.reserved_entity_item(id, idx);
                    if $a.is_some() {
                        mask |= 1 << mi;
                    }
                    mi += 1;
                )+
                if Op::mask(mask, mi) {
                    Some(($($a,)+))
                } else {
                    None
                }
            }
        }

        unsafe impl<Op $(, $a)+> ImmutableQuery for BooleanQuery<($($a,)+), Op>
        where
            $($a: ImmutableQuery,)+
            Op: BooleanFetchOp,
        {
        }

        unsafe impl<Op $(, $a)+> SendQuery for BooleanQuery<($($a,)+), Op>
        where
            $($a: SendQuery,)+
            Op: BooleanFetchOp,
        {
        }
    };
}

for_tuple!(impl_boolean);

/// Combines tuple of filters and yields only entities that pass all of them.
pub type And<T> = BooleanQuery<T, AndOp>;

/// Combines tuple of filters and yields only entities that pass any of them.
pub type Or<T> = BooleanQuery<T, OrOp>;

/// Combines tuple of filters and yields only entities that pass exactly one.
pub type Xor<T> = BooleanQuery<T, XorOp>;

/// Combines two filters and yields only entities that pass all of them.
pub type And2<A, B> = And<(A, B)>;

/// Combines three filters and yields only entities that pass all of them.
pub type And3<A, B, C> = And<(A, B, C)>;

/// Combines four filters and yields only entities that pass all of them.
pub type And4<A, B, C, D> = And<(A, B, C, D)>;

/// Combines five filters and yields only entities that pass all of them.
pub type And5<A, B, C, D, E> = And<(A, B, C, D, E)>;

/// Combines six filters and yields only entities that pass all of them.
pub type And6<A, B, C, D, E, F> = And<(A, B, C, D, E, F)>;

/// Combines seven filters and yields only entities that pass all of them.
pub type And7<A, B, C, D, E, F, G> = And<(A, B, C, D, E, F, G)>;

/// Combines eight filters and yields only entities that pass all of them.
pub type And8<A, B, C, D, E, F, G, H> = And<(A, B, C, D, E, F, G, H)>;

/// Combines two filters and yields only entities that pass any of them.
pub type Or2<A, B> = Or<(A, B)>;

/// Combines three filters and yields only entities that pass any of them.
pub type Or3<A, B, C> = Or<(A, B, C)>;

/// Combines four filters and yields only entities that pass any of them.
pub type Or4<A, B, C, D> = Or<(A, B, C, D)>;

/// Combines five filters and yields only entities that pass any of them.
pub type Or5<A, B, C, D, E> = Or<(A, B, C, D, E)>;

/// Combines six filters and yields only entities that pass any of them.
pub type Or6<A, B, C, D, E, F> = Or<(A, B, C, D, E, F)>;

/// Combines seven filters and yields only entities that pass any of them.
pub type Or7<A, B, C, D, E, F, G> = Or<(A, B, C, D, E, F, G)>;

/// Combines eight filters and yields only entities that pass any of them.
pub type Or8<A, B, C, D, E, F, G, H> = Or<(A, B, C, D, E, F, G, H)>;

/// Combines two filters and yields only entities that pass exactly one.
pub type Xor2<A, B> = Xor<(A, B)>;

/// Combines three filters and yields only entities that pass exactly one.
pub type Xor3<A, B, C> = Xor<(A, B, C)>;

/// Combines four filters and yields only entities that pass exactly one.
pub type Xor4<A, B, C, D> = Xor<(A, B, C, D)>;

/// Combines five filters and yields only entities that pass exactly one.
pub type Xor5<A, B, C, D, E> = Xor<(A, B, C, D, E)>;

/// Combines six filters and yields only entities that pass exactly one.
pub type Xor6<A, B, C, D, E, F> = Xor<(A, B, C, D, E, F)>;

/// Combines seven filters and yields only entities that pass exactly one.
pub type Xor7<A, B, C, D, E, F, G> = Xor<(A, B, C, D, E, F, G)>;

/// Combines eight filters and yields only entities that pass exactly one.
pub type Xor8<A, B, C, D, E, F, G, H> = Xor<(A, B, C, D, E, F, G, H)>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        component::Component,
        query::{Entities, Modified, With},
        world::World,
    };

    #[derive(Component)]
    struct A;
    #[derive(Component)]
    struct B;

    #[test]
    fn entity_filtering_relaxes_only_xor_query_stages() {
        fn check<Q: Query>(
            query: Q,
            world: &World,
            archetype_idx: usize,
            expected_visit: bool,
            expected_item: bool,
        ) {
            let archetype = &world.archetypes()[archetype_idx];
            assert_eq!(query.visit_archetype(archetype), expected_visit);
            assert_eq!(Q::cache_visit_archetype(archetype), Some(expected_visit));
            if !expected_visit {
                return;
            }
            // The test owns the world and all queries only read component epochs.
            unsafe {
                query.access_archetype(archetype, |_, _| {});
                assert!(query.visit_archetype_late(archetype));
                let mut fetch = query.fetch(archetype_idx as u32, archetype, world.epoch());
                assert!(fetch.visit_chunk(0));
                fetch.touch_chunk(0);
                assert_eq!(fetch.visit_item(0), expected_item);
            }
        }

        let mut world = World::new();
        let before = world.epoch();
        world.spawn_empty();
        world.spawn((A,));
        world.spawn((B,));
        world.spawn((A, B));
        for (idx, archetype) in world.archetypes().iter().enumerate() {
            let a = archetype.has_component(crate::type_id::<A>());
            let b = archetype.has_component(crate::type_id::<B>());
            let plain = (With::<A>::default(), With::<B>::default());
            let filtered = (
                Modified::<With<A>>::new(before),
                Modified::<With<B>>::new(before),
            );
            check(And::from_tuple(plain), &world, idx, a && b, a && b);
            check(Or::from_tuple(plain), &world, idx, a || b, a || b);
            check(Xor::from_tuple(plain), &world, idx, a != b, a != b);
            check(And::from_tuple(filtered), &world, idx, a && b, a && b);
            check(Or::from_tuple(filtered), &world, idx, a || b, a || b);
            check(Xor::from_tuple(filtered), &world, idx, a || b, a != b);
        }
    }

    #[test]
    fn boolean_query_forwards_modified_hooks() {
        type Changed = Or2<Modified<With<A>>, Modified<With<B>>>;
        let mut world = World::new();
        let entity = world.spawn((A, B)).id();
        let mut query = <Changed as QueryArg>::new();
        let count =
            |world: &World, query: Changed| world.view::<Entities>().filter(query).iter().count();

        assert_eq!(count(&world, query), 1);
        query.before(&world);
        query.after(&world);
        assert_eq!(count(&world, query), 0);

        world.get::<&mut A>(entity).unwrap();
        assert_eq!(count(&world, query), 1);
        query.after(&world);
        assert_eq!(count(&world, query), 0);

        world.get::<&mut B>(entity).unwrap();
        assert_eq!(count(&world, query), 1);
        query.after(&world);
        assert_eq!(count(&world, query), 0);
    }
}
