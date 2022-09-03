//! This module contains definitions for action recording.
//! Actions can be recorded into [`ActionEncoder`] and executed later onto the [`World`].
//! Two primary use cases for actions are:
//! * Deferring [`World`] mutations when [`World`] is borrowed immutably.
//! * Generating commands in custom component drop-glue.
//!

use core::any::TypeId;

use alloc::collections::VecDeque;

use crate::{
    bundle::{Bundle, DynamicComponentBundle},
    component::Component,
    entity::EntityId,
    world::World,
};

tiny_fn::tiny_fn! {
    struct ActionFn = FnOnce(world: &mut World, encoder: &mut ActionEncoder) | + Send;
}

/// An action that can be recorded by custom drop-glue.
enum Action {
    /// Drops component from the specified entity.
    Remove(EntityId, TypeId),

    /// Despawns specified entity.
    Despawn(EntityId),

    /// Runs a function with the specified entity.
    Fun(ActionFn<'static>),
}

/// Encoder provided to the drop-glue.
/// Custom drop-glue may record drop-actions to it.
#[repr(transparent)]
pub struct ActionEncoder {
    actions: VecDeque<Action>,
}

impl ActionEncoder {
    /// Returns new empty [`ActionEncoder`].
    #[inline]
    pub fn new() -> ActionEncoder {
        ActionEncoder {
            actions: VecDeque::new(),
        }
    }

    /// Returns `true` if action encoder is empty.
    /// That is, no actions were recorded.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Encodes an action to insert components from entity builder to the specified entity.
    #[inline]
    pub fn insert<T>(&mut self, entity: EntityId, component: T)
    where
        T: Component + Send,
    {
        self.custom(move |world, _| {
            let _ = world.insert(entity, component);
        });
    }

    /// Encodes an action to insert components from entity builder to the specified entity.
    #[inline]
    pub fn insert_bundle<B>(&mut self, entity: EntityId, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        self.custom(move |world, _| {
            let _ = world.insert_bundle(entity, bundle);
        });
    }

    /// Encodes an action to remove component from specified entity.
    #[inline]
    pub fn remove_component<T>(&mut self, entity: EntityId) -> &mut Self
    where
        T: Component,
    {
        self.remove_component_raw(entity, TypeId::of::<T>())
    }

    /// Encodes an action to remove component from specified entity.
    #[inline]
    pub fn remove_component_raw(&mut self, entity: EntityId, ty: TypeId) -> &mut Self {
        self.actions.push_back(Action::Remove(entity, ty));
        self
    }

    /// Encodes an action to remove component from specified entity.
    #[inline]
    pub fn remove_bundle<B>(&mut self, entity: EntityId) -> &mut Self
    where
        B: Bundle,
    {
        self.actions.push_back(Action::Fun(ActionFn::new(
            move |world: &mut World, encoder: &mut ActionEncoder| {
                let _ = world.drop_bundle_with_encoder::<B>(entity, encoder);
            },
        )));
        self
    }

    /// Encodes an action to despawn specified entity.
    #[inline]
    pub fn despawn(&mut self, entity: EntityId) -> &mut Self {
        self.actions.push_back(Action::Despawn(entity));
        self
    }

    /// Encodes an action to remove component from specified entity.
    #[inline]
    pub fn custom(
        &mut self,
        fun: impl FnOnce(&mut World, &mut ActionEncoder) + Send + 'static,
    ) -> &mut Self {
        self.actions.push_back(Action::Fun(ActionFn::new(fun)));
        self
    }

    /// Executes recorded commands onto the [`World`].
    /// Iterates through all recorded actions and executes them one by one.
    /// After execution encoder is empty.
    ///
    /// Returns `true` if at least one action was executed.
    #[inline]
    pub fn execute(&mut self, world: &mut World) -> bool {
        if self.actions.is_empty() {
            return false;
        }

        while let Some(action) = self.actions.pop_front() {
            match action {
                Action::Remove(entity, id) => {
                    let _ = world.drop_erased_with_encoder(entity, id, self);
                }
                Action::Despawn(entity) => {
                    let _ = world.despawn_with_encoder(entity, self);
                }
                Action::Fun(fun) => {
                    fun.call(world, self);
                }
            }
        }

        true
    }
}

/// Extension trait for slice of [`ActionEncoder`]s.
pub trait ActionEncoderSliceExt {
    /// Execute all action encoders from the slice.
    /// Returns `true` if at least one action was executed.
    fn execute_all(&mut self, world: &mut World) -> bool;
}

impl ActionEncoderSliceExt for [ActionEncoder] {
    fn execute_all(&mut self, world: &mut World) -> bool {
        self.iter_mut().any(|encoder| encoder.execute(world))
    }
}
