use crate::{
    action::{ActionBuffer, ActionChannel},
    component::{
        Component, ComponentInfo, ComponentInfoRef, ComponentRegistry, ExternalDropHook,
        ExternalSetHook,
    },
    entity::{EntitySet, IdRangeAllocator},
    res::Res,
};

use super::{ArchetypeSet, Edges, EpochCounter, World};

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
            action_buffer: Some(ActionBuffer::new()),
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
