use crate::{
    component::{
        Component, ComponentInfo, ComponentInfoRef, ComponentRegistry, ExternalDropHook,
        ExternalSetHook,
    },
    entity::EntitySet,
    res::Res,
    ActionBuffer,
};

use super::{ArchetypeSet, Edges, EpochCounter, World};

/// Builder for [`World`] value.
///
/// [`WorldBuilder`] allows to perform setup before building [`World`] value.
/// That otherwise would be impossible.
/// For example [`WorldBuilder::register_component`] allows customization of registered components.
pub struct WorldBuilder {
    registry: ComponentRegistry,
}

impl WorldBuilder {
    /// Returns new [`WorldBuilder`] value.
    #[must_use]
    pub const fn new() -> WorldBuilder {
        WorldBuilder {
            registry: ComponentRegistry::new(),
        }
    }

    /// Returns newly created [`World`] with configuration copied from this [`WorldBuilder`].
    #[must_use]
    pub fn build(self) -> World {
        World {
            epoch: EpochCounter::new(),
            entities: EntitySet::new(),
            archetypes: ArchetypeSet::new(),
            edges: Edges::new(),
            res: Res::new(),
            registry: self.registry,
            cached_action_buffer: Some(ActionBuffer::new()),
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
}
