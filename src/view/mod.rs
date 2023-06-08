//! A view over [`World`] that may be used to access specific components.
//!
//! The world can be seen as a table. Then entities would be rows and components would be columns.
//! Then [`View`] is a columnar slice of the table with filtering.

use core::cell::Cell;

use crate::{
    archetype::Archetype,
    entity::EntitySet,
    query::{DefaultQuery, IntoQuery, Query},
    world::World,
};

/// A view borrow state.
///
/// View must borrow components it accesses before
/// dereferencing any pointers.
pub trait BorrowState {
    /// Borrow components in the archetype if not already borrowed.
    fn acquire<Q: Query>(&mut self, query: &Q, archetypes: &[Archetype]);

    /// Release previously acquired borrow.
    fn release<Q: Query>(&mut self, query: &Q, archetypes: &[Archetype]);
}

/// Borrow state for runtime borrowing.
pub struct RuntimeBorrowState {
    borrowed: Cell<bool>,
}

impl RuntimeBorrowState {
    /// Create a new borrow state in the unborrowed state.
    pub const fn new() -> Self {
        RuntimeBorrowState {
            borrowed: Cell::new(false),
        }
    }
}

impl BorrowState for RuntimeBorrowState {
    #[inline]
    fn acquire<Q: Query>(&mut self, query: &Q, archetypes: &[Archetype]) {
        if self.borrowed.get() {
            return;
        }

        struct ReleaseOnFailure<'a, Q: Query> {
            archetypes: &'a [Archetype],
            query: &'a Q,
            len: usize,
        }

        impl<'a, Q> Drop for ReleaseOnFailure<'a, Q>
        where
            Q: Query,
        {
            fn drop(&mut self) {
                for archetype in &self.archetypes[..self.len] {
                    unsafe {
                        if self.query.visit_archetype(archetype) {
                            self.query.access_archetype(archetype, &|id, access| {
                                archetype.component(id).unwrap_unchecked().release(access);
                            });
                        }
                    }
                }
            }
        }

        let mut guard = ReleaseOnFailure {
            archetypes,
            query,
            len: 0,
        };

        for archetype in archetypes {
            unsafe {
                if query.visit_archetype(archetype) {
                    query.access_archetype(archetype, &|id, access| {
                        let success = archetype.component(id).unwrap_unchecked().borrow(access);
                        assert!(success, "Failed to lock '{:?}' from archetype", id);
                    });
                }
            }
            guard.len += 1;
        }

        core::mem::forget(guard);
        self.borrowed.set(true);
    }

    #[inline]
    fn release<Q: Query>(&mut self, query: &Q, archetypes: &[Archetype]) {
        if !self.borrowed.get() {
            return;
        }

        for archetype in archetypes {
            unsafe {
                if query.visit_archetype(archetype) {
                    query.access_archetype(archetype, &|id, access| {
                        archetype.component(id).unwrap_unchecked().release(access);
                    });
                }
            }
        }
    }
}

/// Borrow state for statically borrowed views.
/// These can be created from [`&mut World`](World)
/// or unsafely from [`&World`](World).
pub struct StaticallyBorrowed;

impl BorrowState for StaticallyBorrowed {
    #[inline(always)]
    fn acquire<Q: Query>(&mut self, _query: &Q, _archetypes: &[Archetype]) {}

    #[inline(always)]
    fn release<Q: Query>(&mut self, _query: &Q, _archetypes: &[Archetype]) {}
}

/// A helper trait to extend tuples.
pub trait ExtendTuple<E>: Sized {
    /// Tuple with an additional element `E`.
    type Output;

    /// Extend tuple with an additional element `E`.
    fn extend_tuple(self, element: E) -> Self::Output;
}

/// A helper type alias to extend tuples.
pub type TuplePlus<T, E> = <T as ExtendTuple<E>>::Output;

macro_rules! impl_extend {
    ($($a:ident)*) => {
        impl<Add $(, $a)*> ExtendTuple<Add> for ($($a,)*)
        {
            type Output = ($($a,)* Add,);

            #[inline]
            fn extend_tuple(self, add: Add) -> Self::Output {
                #![allow(non_snake_case)]
                let ($($a,)*) = self;
                ($($a,)* add,)
            }
        }
    };
}

for_tuple!(impl_extend);

/// A view over [`World`] that may be used to access specific components.
pub struct View<'a, Q: IntoQuery, F: IntoQuery = (), B: BorrowState = StaticallyBorrowed> {
    query: Q::Query,
    filter: F::Query,
    archetypes: &'a [Archetype],
    entity_set: &'a EntitySet,
    borrow_state: B,
}

impl<'a, Q, F> View<'a, Q, F>
where
    Q: DefaultQuery,
    F: DefaultQuery,
{
    /// Creates a new view over the world.
    /// Borrows the world mutably, so no other views can be created.
    /// In exchange it does not require runtime borrow checks.
    ///
    /// Uses default-constructed query and filter.
    ///
    /// # Example
    ///
    /// ```
    /// use edict::{component::Component, view::View, world::World};
    ///
    /// #[derive(Component)]
    /// struct Foo;
    ///
    /// let mut world = World::new();
    /// world.spawn((Foo,));
    ///
    /// let view = View::<&Foo>::new(&mut world);
    ///
    /// for (foo,) in view.iter() {
    ///     println!("Found Foo!");
    /// }
    /// ```
    #[inline]
    pub fn new_mut(world: &'a mut World) -> Self {
        View {
            query: Q::default_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: StaticallyBorrowed,
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses default-constructed query and filter.
    #[inline]
    pub unsafe fn new_unchecked(world: &'a World) -> Self {
        View {
            query: Q::default_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: StaticallyBorrowed,
        }
    }
}

impl<'a, Q, F> View<'a, Q, F>
where
    Q: IntoQuery,
    F: DefaultQuery,
{
    /// Creates a new view over the world.
    /// Borrows the world mutably, so no other views can be created.
    /// In exchange it does not require runtime borrow checks.
    ///
    /// Uses user-provided query and default-constructed filter.
    #[inline]
    pub fn with_query_mut(world: &'a mut World, query: Q) -> Self {
        View {
            query: query.into_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: StaticallyBorrowed,
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses user-provided query and default-constructed filter.
    #[inline]
    pub unsafe fn with_query_unchecked(world: &'a World, query: Q) -> Self {
        View {
            query: query.into_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: StaticallyBorrowed,
        }
    }
}

impl<'a, Q, F> View<'a, Q, F>
where
    Q: IntoQuery,
    F: IntoQuery,
{
    /// Creates a new view over the world.
    /// Borrows the world mutably, so no other views can be created.
    /// In exchange it does not require runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline]
    pub fn with_query_filter_mut(world: &'a mut World, query: Q, filter: F) -> Self {
        View {
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: StaticallyBorrowed,
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline]
    pub unsafe fn with_query_filter_unchecked(world: &'a World, query: Q, filter: F) -> Self {
        View {
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: StaticallyBorrowed,
        }
    }
}

impl<'a, Q, F> View<'a, Q, F, RuntimeBorrowState>
where
    Q: DefaultQuery,
    F: DefaultQuery,
{
    /// Creates a new view over the world.
    /// Performs runtime borrow checks.
    ///
    /// Uses default-constructed query and filter.
    #[inline]
    pub fn new(world: &'a World) -> Self {
        View {
            query: Q::default_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: RuntimeBorrowState::new(),
        }
    }
}

impl<'a, Q, F> View<'a, Q, F, RuntimeBorrowState>
where
    Q: IntoQuery,
    F: DefaultQuery,
{
    /// Creates a new view over the world.
    /// Performs runtime borrow checks.
    ///
    /// Uses user-provided query and default-constructed filter.
    #[inline]
    pub fn with_query(world: &'a World, query: Q) -> Self {
        View {
            query: query.into_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: RuntimeBorrowState::new(),
        }
    }
}

impl<'a, Q, F> View<'a, Q, F, RuntimeBorrowState>
where
    Q: IntoQuery,
    F: IntoQuery,
{
    /// Creates a new view over the world.
    /// Performs runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline]
    pub fn with_query_filter(world: &'a mut World, query: Q, filter: F) -> Self {
        View {
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow_state: RuntimeBorrowState::new(),
        }
    }
}

impl<'a, Q, F, B> View<'a, Q, F, B>
where
    Q: IntoQuery,
    F: IntoQuery,
    B: BorrowState,
{
    /// Fetches data from the view for the given entity.
    pub fn get(&mut self, entity: Entity);
}
