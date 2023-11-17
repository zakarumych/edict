use alloc::boxed::Box;
use core::marker::PhantomData;

use crate::{
    action::{ActionChannel, LocalActionBuffer},
    bundle::ComponentBundle,
    component::{
        Component, ComponentInfo, ComponentInfoRef, ComponentRegistry, ExternalDropHook,
        ExternalSetHook,
    },
    entity::{EntitySet, IdRangeAllocator},
    res::Res,
};

use super::{register_bundle, ArchetypeSet, Edges, EpochCounter, World};

/// Builder for [`World`] value.
///
/// [`WorldBuilder`] allows to perform setup before building [`World`] value.
/// That otherwise would be impossible.
/// For example [`WorldBuilder::register_component`] allows customization of registered components.
pub struct WorldBuilder {
    registry: ComponentRegistry,
    range_alloc: Option<Box<dyn IdRangeAllocator>>,
}

impl WorldBuilder {
    /// Returns new [`WorldBuilder`] value.
    #[must_use]
    pub const fn new() -> WorldBuilder {
        WorldBuilder {
            registry: ComponentRegistry::new(),
            range_alloc: None,
        }
    }

    /// Returns newly created [`World`] with configuration copied from this [`WorldBuilder`].
    #[must_use]
    pub fn build(self) -> World {
        let entities = match self.range_alloc {
            None => EntitySet::new(),
            Some(range_alloc) => EntitySet::with_allocator(range_alloc),
        };

        World {
            epoch: EpochCounter::new(),
            entities,
            archetypes: ArchetypeSet::new(),
            edges: Edges::new(),
            res: Res::new(),
            registry: self.registry,
            action_buffer: LocalActionBuffer::new(),
            action_channel: ActionChannel::new(),
        }
    }

    /// Registers new component type and allows modifying it.
    pub fn register_raw(&mut self, info: ComponentInfo) {
        self.registry.register_raw(info);
    }

    /// Registers new component type and allows modifying it.
    pub fn register_component<T>(&mut self) -> ComponentInfoRef<'_, T>
    where
        T: Component,
    {
        self.registry.register_component::<T>()
    }

    /// Registers new component type and allows modifying it.
    pub fn register_external<T>(
        &mut self,
    ) -> ComponentInfoRef<'_, T, ExternalDropHook, ExternalSetHook>
    where
        T: 'static,
    {
        self.registry.register_external::<T>()
    }

    /// Sets custom ID range allocator to be used by the [`World`].
    /// Replaces previously set allocator.
    /// If no allocator is set, no range allocator is used
    /// and [`World`] will allocate sequentially all IDs in range [1..=u64::MAX].
    ///
    /// If allocator is set, [`World`] will allocate IDs from ranges provided by the allocator.
    /// If allocator is exhausted, allocating new entities will panic.
    pub fn with_id_range_allocator(mut self, range_alloc: Box<dyn IdRangeAllocator>) -> Self {
        self.range_alloc = Some(range_alloc);
        self
    }
}

impl World {
    /// Returns new instance of [`World`].
    /// Created [`World`] instance contains no entities.
    #[inline(always)]
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Returns new instance of [`WorldBuilder`].
    /// This allows pre-register component types and override their behavior.
    #[inline(always)]
    pub const fn builder() -> WorldBuilder {
        WorldBuilder::new()
    }

    /// Explicitly registers component type.
    ///
    /// Unlike [`WorldBuilder::register_component`] method, this method does not return reference to component configuration,
    /// once [`World`] is created overriding component behavior is not possible.
    ///
    /// Component types are implicitly registered on first use by most methods.
    /// This method is only needed if you want to use component type using
    /// [`World::insert_external`], [`World::insert_external_bundle`] or [`World::spawn_external`].
    pub fn ensure_component_registered<T>(&mut self)
    where
        T: Component,
    {
        self.registry.ensure_component_registered::<T>();
    }

    /// Explicitly registers bundle of component types.
    ///
    /// This method is only needed if you want to use bundle of component types using
    /// [`World::insert_external_bundle`] or [`World::spawn_external`].
    pub fn ensure_bundle_registered<B>(&mut self)
    where
        B: ComponentBundle,
    {
        register_bundle(&mut self.registry, &PhantomData::<B>);
    }

    /// Explicitly registers external type.
    ///
    /// Unlike [`WorldBuilder::register_external`] method, this method does not return reference to component configuration,
    /// once [`World`] is created overriding component behavior is not possible.
    ///
    /// External component types are not implicitly registered on first use.
    /// This method is needed if you want to use component type with
    /// [`World::insert_external`], [`World::insert_external_bundle`] or [`World::spawn_external`].
    pub fn ensure_external_registered<T>(&mut self)
    where
        T: 'static,
    {
        self.registry.ensure_external_registered::<T>();
    }
}
