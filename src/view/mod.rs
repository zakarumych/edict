//! A view over [`World`] that may be used to access specific components.
//!
//! The world can be seen as a table. Then entities would be rows and components would be columns.
//! Then [`View`] is a columnar slice of the table with filtering.

use crate::{
    archetype::Archetype,
    entity::EntitySet,
    epoch::EpochCounter,
    query::{DefaultQuery, IntoQuery, Query},
    world::World,
};

pub use self::borrow::{BorrowState, RuntimeBorrowState, StaticallyBorrowed};

mod borrow;
mod extend;
mod index;
mod iter;
mod one;

/// A view over [`World`] that may be used to access specific components.
#[must_use]
pub struct ViewState<'a, Q: Query, F: Query, B> {
    query: Q,
    filter: F,
    archetypes: &'a [Archetype],
    entity_set: &'a EntitySet,
    borrow: B,
    epochs: &'a EpochCounter,
}

pub type View<'a, Q, F = (), B = RuntimeBorrowState> =
    ViewState<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query, B>;

pub type ViewMut<'a, Q, F = ()> =
    ViewState<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query, StaticallyBorrowed>;

impl<'a, Q, F> ViewMut<'a, Q, F>
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
    /// use edict::{component::Component, view::ViewMut, world::World};
    ///
    /// #[derive(Component)]
    /// struct Foo;
    ///
    /// let mut world = World::new();
    /// world.spawn((Foo,));
    ///
    /// let view = ViewMut::<&Foo>::new_mut(&mut world);
    ///
    /// for (foo,) in view.iter() {
    ///     println!("Found Foo!");
    /// }
    /// ```
    #[inline]
    pub fn new_mut(world: &'a mut World) -> Self {
        ViewMut {
            query: Q::default_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: StaticallyBorrowed,
            epochs: world.epoch_counter(),
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses default-constructed query and filter.
    #[inline]
    pub unsafe fn new_unchecked(world: &'a World) -> Self {
        ViewMut {
            query: Q::default_query(),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: StaticallyBorrowed,
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewMut<'a, (Q,), F>
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
        ViewMut {
            query: (query.into_query(),),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: StaticallyBorrowed,
            epochs: world.epoch_counter(),
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses user-provided query and default-constructed filter.
    #[inline]
    pub unsafe fn with_query_unchecked(world: &'a World, query: Q) -> Self {
        ViewMut {
            query: (query.into_query(),),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: StaticallyBorrowed,
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewMut<'a, (Q,), (F,)>
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
        ViewMut {
            query: (query.into_query(),),
            filter: (filter.into_query(),),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: StaticallyBorrowed,
            epochs: world.epoch_counter(),
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline]
    pub unsafe fn with_query_filter_unchecked(world: &'a World, query: Q, filter: F) -> Self {
        ViewMut {
            query: (query.into_query(),),
            filter: (filter.into_query(),),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: StaticallyBorrowed,
            epochs: world.epoch_counter(),
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
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> View<'a, (Q,), F, RuntimeBorrowState>
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
            query: (query.into_query(),),
            filter: F::default_query(),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> View<'a, (Q,), (F,), RuntimeBorrowState>
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
            query: (query.into_query(),),
            filter: (filter.into_query(),),
            archetypes: world.archetypes(),
            entity_set: world.entity_set(),
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}
