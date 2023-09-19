//! This module contains `Archetype` type definition.

use alloc::{
    alloc::{alloc, dealloc},
    boxed::Box,
    vec::Vec,
};
use core::{
    alloc::Layout,
    any::TypeId,
    cell::UnsafeCell,
    hint::unreachable_unchecked,
    intrinsics::copy_nonoverlapping,
    iter::FromIterator,
    mem::{self, size_of, MaybeUninit},
    ops::Deref,
    ptr::{self, NonNull},
};

use atomicell::borrow::{
    new_lock, release_borrow, release_borrow_mut, try_borrow, try_borrow_mut, Lock,
};
use hashbrown::HashMap;

use crate::{
    action::ActionEncoder, bundle::DynamicBundle, component::ComponentInfo, entity::EntityId,
    epoch::EpochId, hash::NoOpHasherBuilder, idx::MAX_IDX_USIZE, Access,
};

pub(crate) struct ComponentData {
    pub ptr: NonNull<u8>,
    pub epoch: EpochId,
    pub entity_epochs: Box<[EpochId]>,
    pub chunk_epochs: Box<[EpochId]>,
}

pub(crate) struct ArchetypeComponent {
    info: ComponentInfo,
    lock: Lock,
    data: UnsafeCell<ComponentData>,
}

impl Deref for ArchetypeComponent {
    type Target = ComponentInfo;

    fn deref(&self) -> &ComponentInfo {
        &self.info
    }
}

impl ArchetypeComponent {
    #[inline(always)]
    pub unsafe fn borrow(&self, access: Access) -> bool {
        match access {
            Access::Read => try_borrow(&self.lock),
            Access::Write => try_borrow_mut(&self.lock),
        }
    }

    #[inline(always)]
    pub unsafe fn release(&self, access: Access) {
        match access {
            Access::Read => release_borrow(&self.lock),
            Access::Write => release_borrow_mut(&self.lock),
        }
    }

    #[inline(always)]
    pub unsafe fn data(&self) -> &ComponentData {
        unsafe { &*self.data.get() }
    }

    #[inline(always)]
    pub unsafe fn data_mut(&self) -> &mut ComponentData {
        unsafe { &mut *self.data.get() }
    }
}

impl ArchetypeComponent {
    fn new(info: &ComponentInfo) -> Self {
        ArchetypeComponent {
            data: UnsafeCell::new(ComponentData {
                ptr: NonNull::dangling(),
                epoch: EpochId::start(),
                chunk_epochs: Box::new([]),
                entity_epochs: Box::new([]),
            }),
            lock: new_lock(),
            info: info.clone(),
        }
    }

    unsafe fn drop(&mut self, cap: usize, len: usize) {
        let data = self.data.get_mut();

        self.info.final_drop(data.ptr, len);

        if self.info.layout().size() != 0 && cap != 0 {
            // Safety: layout of existing allocation.
            let layout = unsafe {
                Layout::from_size_align_unchecked(
                    self.info.layout().size() * cap,
                    self.info.layout().align(),
                )
            };

            unsafe {
                dealloc(data.ptr.as_ptr(), layout);
            }
        }
    }

    unsafe fn grow(&mut self, len: u32, old_cap: u32, new_cap: u32) {
        let data = self.data.get_mut();

        debug_assert!(len <= old_cap);
        debug_assert!(old_cap < new_cap);

        if self.info.layout().size() != 0 {
            let new_layout = Layout::from_size_align(
                self.info
                    .layout()
                    .size()
                    .checked_mul(new_cap as usize)
                    .unwrap(),
                self.info.layout().align(),
            )
            .unwrap();

            // # Safety: component size is non-zero, new_cap is non-zero.
            // Thus new_layout size is non-zero.
            let Some(mut ptr) = NonNull::new(unsafe { alloc(new_layout) }) else {
                alloc::alloc::handle_alloc_error(new_layout);
            };

            if len != 0 {
                unsafe {
                    copy_nonoverlapping(
                        data.ptr.as_ptr(),
                        ptr.as_ptr(),
                        (len as usize) * self.info.layout().size(),
                    )
                };
            }

            if old_cap != 0 {
                // Safety: layout of existing allocation.
                let old_layout = unsafe {
                    Layout::from_size_align_unchecked(
                        self.info.layout().size() * (old_cap as usize),
                        self.info.layout().align(),
                    )
                };

                mem::swap(&mut data.ptr, &mut ptr);

                unsafe {
                    dealloc(ptr.as_ptr(), old_layout);
                }
            } else {
                data.ptr = ptr;
            }
        }

        let mut entity_epochs = core::mem::take(&mut data.entity_epochs).into_vec();
        entity_epochs.reserve_exact((new_cap - old_cap) as usize);
        entity_epochs.resize(new_cap as usize, EpochId::start());
        data.entity_epochs = entity_epochs.into_boxed_slice();

        let mut chunk_epochs = core::mem::take(&mut data.chunk_epochs).into_vec();
        chunk_epochs.reserve_exact((chunks_count(new_cap) - chunks_count(old_cap)) as usize);
        chunk_epochs.resize(chunks_count(new_cap) as usize, EpochId::start());
        data.chunk_epochs = chunk_epochs.into_boxed_slice();
    }
}

/// Collection of all entities with same set of components.
/// Archetypes are typically managed by the `World` instance.
///
/// This type is exposed for `Query` implementations.
pub struct Archetype {
    entities: Vec<EntityId>,
    components: HashMap<TypeId, ArchetypeComponent, NoOpHasherBuilder>,
    borrows: HashMap<TypeId, Vec<(TypeId, usize)>, NoOpHasherBuilder>,
    borrows_mut: HashMap<TypeId, Vec<(TypeId, usize)>, NoOpHasherBuilder>,
}

impl Drop for Archetype {
    fn drop(&mut self) {
        for (_, c) in &mut self.components {
            unsafe {
                c.drop(self.entities.capacity(), self.entities.len());
            }
        }
    }
}

impl Archetype {
    /// Creates new archetype with the given set of components.
    pub fn new<'a>(components: impl Iterator<Item = &'a ComponentInfo> + Clone) -> Self {
        let components = HashMap::from_iter(components.map(|c| {
            let c = ArchetypeComponent::new(c);
            (c.id(), c)
        }));

        let mut borrows = HashMap::with_hasher(NoOpHasherBuilder);
        let mut borrows_mut = HashMap::with_hasher(NoOpHasherBuilder);

        for (&id, c) in &components {
            for (idx, cb) in c.borrows().iter().enumerate() {
                borrows
                    .entry(cb.target())
                    .or_insert_with(Vec::new)
                    .push((id, idx));

                if cb.has_borrow_mut() {
                    borrows_mut
                        .entry(cb.target())
                        .or_insert_with(Vec::new)
                        .push((id, idx));
                }
            }
        }

        Archetype {
            entities: Vec::new(),
            components,
            borrows,
            borrows_mut,
        }
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline(always)]
    pub fn has_component(&self, type_id: TypeId) -> bool {
        self.components.contains_key(&type_id)
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline(always)]
    pub fn contains_borrow(&self, type_id: TypeId) -> bool {
        self.borrows.contains_key(&type_id)
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline(always)]
    pub fn contains_borrow_mut(&self, type_id: TypeId) -> bool {
        self.borrows_mut.contains_key(&type_id)
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    #[inline(always)]
    pub(crate) fn borrow_indices(&self, type_id: TypeId) -> Option<&[(TypeId, usize)]> {
        self.borrows.get(&type_id).map(|v| &v[..])
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    #[inline(always)]
    pub(crate) fn borrow_mut_indices(&self, type_id: TypeId) -> Option<&[(TypeId, usize)]> {
        self.borrows_mut.get(&type_id).map(|v| &v[..])
    }

    /// Returns `true` if archetype matches components set specified.
    #[inline(always)]
    pub fn matches(&self, mut type_ids: impl Iterator<Item = TypeId>) -> bool {
        let len = self.components.len();
        match type_ids.size_hint() {
            (l, u) if l <= len && u.map_or(true, |u| u >= len) => {
                type_ids.try_fold(0usize, |count, type_id| {
                    if self.components.contains_key(&type_id) {
                        Some(count + 1)
                    } else {
                        None
                    }
                }) == Some(len)
            }
            _ => false,
        }
    }

    /// Returns iterator over component type ids.
    #[inline(always)]
    pub fn ids(&self) -> impl ExactSizeIterator<Item = TypeId> + Clone + '_ {
        self.components.keys().copied()
    }

    /// Returns iterator over component type infos.
    #[inline(always)]
    pub fn infos(&self) -> impl ExactSizeIterator<Item = &'_ ComponentInfo> + Clone + '_ {
        self.components.iter().map(|(_, c)| &c.info)
    }

    /// Spawns new entity in the archetype.
    ///
    /// Returns index of the newly created entity in the archetype.
    pub fn spawn<B>(&mut self, id: EntityId, bundle: B, epoch: EpochId) -> u32
    where
        B: DynamicBundle,
    {
        debug_assert!(bundle.with_ids(|ids| self.matches(ids.iter().copied())));
        debug_assert!(self.entities.len() < MAX_IDX_USIZE);

        let entity_idx = self.entities.len() as u32;

        unsafe {
            self.reserve(1);

            debug_assert_ne!(self.entities.len(), self.entities.capacity());
            self.write_bundle(id, entity_idx, bundle, epoch, None, |_| false);
        }

        self.entities.push(id);
        entity_idx as u32
    }

    /// Despawns specified entity in the archetype.
    ///
    /// Returns id of the entity that took the place of despawned.
    #[inline(always)]
    pub fn despawn(&mut self, id: EntityId, idx: u32, encoder: ActionEncoder) -> Option<EntityId> {
        assert!(idx < self.entities.len() as u32);

        unsafe { self.despawn_unchecked(id, idx, encoder) }
    }

    /// Despawns specified entity in the archetype.
    ///
    /// Returns id of the entity that took the place of despawned.
    ///
    /// # Safety
    ///
    /// idx must be in bounds of the archetype entities array.
    pub unsafe fn despawn_unchecked(
        &mut self,
        id: EntityId,
        idx: u32,
        mut encoder: ActionEncoder,
    ) -> Option<EntityId> {
        let entity_idx = idx;
        debug_assert!(entity_idx < self.entities.len() as u32);
        debug_assert_eq!(id, self.entities[entity_idx as usize]);

        let last_entity_idx = (self.entities.len() - 1) as u32;

        for component in self.components.values_mut() {
            let data = component.data.get_mut();
            let size = component.info.layout().size();

            // Safety: ptr within the allocation block.
            // Or dangling if size is 0, but than result equals `data.ptr`
            let ptr = unsafe {
                NonNull::new_unchecked(data.ptr.as_ptr().add((entity_idx as usize) * size))
            };

            component.info.drop_one(ptr, id, encoder.reborrow());

            if entity_idx != last_entity_idx {
                let chunk_idx = chunk_idx(entity_idx);

                let last_epoch =
                    unsafe { *data.entity_epochs.as_ptr().add(last_entity_idx as usize) };

                let chunk_epoch =
                    unsafe { data.chunk_epochs.get_unchecked_mut(chunk_idx as usize) };
                let entity_epoch =
                    unsafe { data.entity_epochs.get_unchecked_mut(entity_idx as usize) };

                chunk_epoch.update(last_epoch);
                *entity_epoch = last_epoch;

                let last_ptr = unsafe { data.ptr.as_ptr().add((last_entity_idx as usize) * size) };
                unsafe {
                    ptr::copy_nonoverlapping(last_ptr, ptr.as_ptr(), size);
                }
            }

            #[cfg(debug_assertions)]
            unsafe {
                *data
                    .entity_epochs
                    .get_unchecked_mut(last_entity_idx as usize) = EpochId::start();
            }
        }

        self.entities.swap_remove(entity_idx as usize);
        if entity_idx != last_entity_idx {
            Some(self.entities[entity_idx as usize])
        } else {
            None
        }
    }

    /// Set components from bundle to the entity.
    ///
    /// # Safety
    ///
    /// Bundle must not contain components that are absent in this archetype.
    pub unsafe fn set_bundle<B>(
        &mut self,
        id: EntityId,
        idx: u32,
        bundle: B,
        epoch: EpochId,
        encoder: ActionEncoder,
    ) where
        B: DynamicBundle,
    {
        let entity_idx = idx;
        debug_assert!(
            bundle.with_ids(|ids| ids.iter().all(|&id| self.components.contains_key(&id)))
        );
        debug_assert!(entity_idx < self.entities.len() as u32);

        unsafe {
            self.write_bundle(id, entity_idx, bundle, epoch, Some(encoder), |_| true);
        }
    }

    /// Set component to the entity
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    #[inline(always)]
    pub unsafe fn set<T>(
        &mut self,
        id: EntityId,
        idx: u32,
        value: T,
        epoch: EpochId,
        encoder: ActionEncoder,
    ) where
        T: 'static,
    {
        let entity_idx = idx;

        debug_assert!(self.components.contains_key(&TypeId::of::<T>()));
        debug_assert!(entity_idx < self.entities.len() as u32);

        unsafe {
            self.write_one(id, entity_idx, value, epoch, Some(encoder));
        }
    }

    /// Get component of the entity
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    #[inline(always)]
    pub unsafe fn get<T>(&mut self, entity_idx: u32) -> &T
    where
        T: 'static,
    {
        debug_assert!(self.components.contains_key(&TypeId::of::<T>()));
        debug_assert!(entity_idx < self.entities.len() as u32);

        let component = unsafe {
            self.components
                .get_mut(&TypeId::of::<T>())
                .unwrap_unchecked()
        };
        let ptr = unsafe {
            component
                .data
                .get_mut()
                .ptr
                .as_ptr()
                .cast::<T>()
                .add(entity_idx as usize)
        };
        unsafe { &*ptr }
    }

    /// Borrows component mutably. Updates entity epoch.
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    /// `epoch` must be advanced before this call.
    #[inline(always)]
    pub unsafe fn get_mut<T>(&mut self, entity_idx: u32, epoch: EpochId) -> &mut T
    where
        T: 'static,
    {
        let chunk_idx = chunk_idx(entity_idx);

        debug_assert!(self.components.contains_key(&TypeId::of::<T>()));
        debug_assert!(entity_idx < self.entities.len() as u32);

        let component = unsafe {
            self.components
                .get_mut(&TypeId::of::<T>())
                .unwrap_unchecked()
        };
        let data = component.data.get_mut();
        let ptr = unsafe { data.ptr.as_ptr().cast::<T>().add(entity_idx as usize) };

        let chunk_epoch = unsafe { data.chunk_epochs.get_unchecked_mut(chunk_idx as usize) };
        let entity_epoch = unsafe { data.entity_epochs.get_unchecked_mut(entity_idx as usize) };

        // `epoch` must be advanced in `World` before this call.
        data.epoch.bump(epoch);
        chunk_epoch.bump(epoch);
        entity_epoch.bump(epoch);

        unsafe { &mut *ptr }
    }

    /// Add components from bundle to the entity, moving entity to new archetype.
    ///
    /// # Safety
    ///
    /// `src_idx` must be in bounds of this archetype.
    /// This archetype must not contain at least one component type from the bundle.
    /// `dst` archetype must contain all component types from this archetype and the bundle.
    pub unsafe fn insert_bundle<B>(
        &mut self,
        id: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
        bundle: B,
        epoch: EpochId,
        encoder: ActionEncoder,
    ) -> (u32, Option<EntityId>)
    where
        B: DynamicBundle,
    {
        debug_assert!(self.ids().all(|id| dst.components.contains_key(&id)));
        debug_assert!(bundle.with_ids(|ids| ids.iter().all(|&id| dst.components.contains_key(&id))));

        debug_assert_eq!(
            bundle.with_ids(|ids| {
                ids.iter()
                    .filter(|&id| !self.components.contains_key(id))
                    .count()
            }) + self.components.len(),
            dst.components.len()
        );

        let src_entity_idx = src_idx;

        debug_assert!(src_entity_idx < self.entities.len() as u32);
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let dst_entity_idx = dst.entities.len() as u32;

        dst.reserve(1);

        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());
        unsafe {
            self.relocate_components(src_entity_idx, dst, dst_entity_idx, |_, _| {
                unreachable_unchecked()
            });
        }

        unsafe {
            dst.write_bundle(id, dst_entity_idx, bundle, epoch, Some(encoder), |id| {
                if self.components.contains_key(&id) {
                    true
                } else {
                    false
                }
            });
        }

        let entity = self.entities.swap_remove(src_entity_idx as usize);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() as u32 {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx as usize]),
            )
        } else {
            (dst_entity_idx as u32, None)
        }
    }

    /// Add one component to the entity moving it to new archetype.
    ///
    /// # Safety
    ///
    /// `src_idx` must be in bounds of this archetype.
    /// This archetype must not contain specified type.
    /// `dst` archetype must contain all component types from this archetype and specified type.
    pub(crate) unsafe fn insert<T>(
        &mut self,
        id: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
        value: T,
        epoch: EpochId,
    ) -> (u32, Option<EntityId>)
    where
        T: 'static,
    {
        debug_assert!(self.ids().all(|id| dst.components.contains_key(&id)));
        debug_assert!(!self.components.contains_key(&TypeId::of::<T>()));
        debug_assert!(dst.components.contains_key(&TypeId::of::<T>()));
        debug_assert_eq!(self.components.len() + 1, dst.components.len());

        let src_entity_idx = src_idx;
        debug_assert!(src_entity_idx < self.entities.len() as u32);

        let dst_entity_idx = dst.entities.len() as u32;

        dst.reserve(1);

        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());
        unsafe {
            self.relocate_components(src_entity_idx, dst, dst_entity_idx, |_, _| {
                unreachable_unchecked()
            });
        }

        unsafe {
            dst.write_one::<T>(id, dst_entity_idx, value, epoch, None);
        }

        let entity = self.entities.swap_remove(src_entity_idx as usize);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() as u32 {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx as usize]),
            )
        } else {
            (dst_entity_idx as u32, None)
        }
    }

    /// Removes one component from the entity moving it to new archetype.
    ///
    /// # Safety
    ///
    /// `src_idx` must be in bounds of this archetype.
    /// This archetype must contain specified type.
    /// `dst` archetype must contain all component types from this archetype except specified type.
    pub unsafe fn remove<T>(
        &mut self,
        id: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
    ) -> (u32, Option<EntityId>, T)
    where
        T: 'static,
    {
        debug_assert!(dst.ids().all(|id| self.components.contains_key(&id)));
        debug_assert!(!dst.components.contains_key(&TypeId::of::<T>()));
        debug_assert!(self.components.contains_key(&TypeId::of::<T>()));
        debug_assert_eq!(dst.components.len() + 1, self.components.len());

        let src_entity_idx = src_idx;
        debug_assert!(src_entity_idx < self.entities.len() as u32);
        debug_assert_eq!(id, self.entities[src_entity_idx as usize]);

        let dst_entity_idx = dst.entities.len() as u32;

        let mut value = MaybeUninit::uninit();

        dst.reserve(1);

        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());
        unsafe {
            self.relocate_components(src_entity_idx, dst, dst_entity_idx, |info, ptr| {
                if info.id() != TypeId::of::<T>() {
                    unreachable_unchecked()
                }

                value.write(ptr::read(ptr.as_ptr().cast()));
            });
        }

        let entity = self.entities.swap_remove(src_entity_idx as usize);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() as u32 {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx as usize]),
                unsafe { value.assume_init() },
            )
        } else {
            (dst_entity_idx as u32, None, unsafe { value.assume_init() })
        }
    }

    /// Moves entity from one archetype to another.
    /// Dropping components types that are not present in dst archetype.
    /// All components present in dst archetype must be present in src archetype.
    ///
    /// # Safety
    ///
    /// `src_idx` must be in bounds of this archetype.
    /// `dst` archetype must contain all component types from this archetype except types from bundle.
    pub unsafe fn drop_bundle(
        &mut self,
        id: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
        mut encoder: ActionEncoder,
    ) -> (u32, Option<EntityId>) {
        debug_assert!(dst.ids().all(|id| self.components.contains_key(&id)));

        let src_entity_idx = src_idx;
        debug_assert!(src_entity_idx < self.entities.len() as u32);
        debug_assert_eq!(id, self.entities[src_entity_idx as usize]);

        let dst_entity_idx = dst.entities.len() as u32;

        dst.reserve(1);
        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());

        unsafe {
            self.relocate_components(src_entity_idx, dst, dst_entity_idx, |info, ptr| {
                info.drop_one(ptr, id, encoder.reborrow());
            });
        }

        let entity = self.entities.swap_remove(src_entity_idx as usize);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() as u32 {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx as usize]),
            )
        } else {
            (dst_entity_idx as u32, None)
        }
    }

    #[inline(always)]
    pub(crate) fn entities(&self) -> &[EntityId] {
        &self.entities
    }

    /// Returns archetype component
    #[inline(always)]
    pub(crate) fn component(&self, ty: TypeId) -> Option<&ArchetypeComponent> {
        self.components.get(&ty)
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.entities.len()
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    #[inline(always)]
    pub(crate) fn reserve(&mut self, additional: u32) {
        debug_assert!(self.entities.len() <= u32::MAX as usize);

        // Pretend that `Vec` can't hold more than `u32::MAX` elements.
        let old_cap = self.entities.capacity().min(u32::MAX as usize) as u32;
        let len = self.entities.len() as u32;

        if additional <= old_cap - len {
            return;
        }

        let req_cap = len.saturating_add(additional);

        // Needs to grow.
        // Saturate at `u32::MAX` elements.
        self.entities.reserve((req_cap - len) as usize);
        let new_cap = self.entities.capacity().min(u32::MAX as usize) as u32;

        for component in self.components.values_mut() {
            unsafe {
                component.grow(len, old_cap, new_cap);
            }
        }
    }

    #[inline(always)]
    unsafe fn write_bundle<B, F>(
        &mut self,
        id: EntityId,
        entity_idx: u32,
        bundle: B,
        epoch: EpochId,
        mut encoder: Option<ActionEncoder>,
        occupied: F,
    ) where
        B: DynamicBundle,
        F: Fn(TypeId) -> bool,
    {
        let chunk_idx = chunk_idx(entity_idx);

        bundle.put(|src, tid, size| {
            let component = unsafe { self.components.get_mut(&tid).unwrap_unchecked() };
            let data = component.data.get_mut();
            let chunk_epoch = unsafe { data.chunk_epochs.get_unchecked_mut(chunk_idx as usize) };
            let entity_epoch = unsafe { data.entity_epochs.get_unchecked_mut(entity_idx as usize) };

            data.epoch.bump_again(epoch); // Batch spawn would happen with same epoch.
            chunk_epoch.bump_again(epoch); // Batch spawn would happen with same epoch.
            entity_epoch.bump(epoch);

            let dst = unsafe {
                NonNull::new_unchecked(data.ptr.as_ptr().add((entity_idx as usize) * size))
            };
            if occupied(tid) {
                component.set_one(dst, src, id, encoder.as_mut().unwrap().reborrow());
            } else {
                unsafe {
                    ptr::copy_nonoverlapping(src.as_ptr(), dst.as_ptr(), size);
                }
            }
        });
    }

    #[inline(always)]
    unsafe fn write_one<T>(
        &mut self,
        id: EntityId,
        entity_idx: u32,
        value: T,
        epoch: EpochId,
        occupied: Option<ActionEncoder>,
    ) where
        T: 'static,
    {
        let chunk_idx = chunk_idx(entity_idx);

        let component = unsafe {
            self.components
                .get_mut(&TypeId::of::<T>())
                .unwrap_unchecked()
        };
        let data = component.data.get_mut();
        let chunk_epoch = unsafe { data.chunk_epochs.get_unchecked_mut(chunk_idx as usize) };
        let entity_epoch = unsafe { data.entity_epochs.get_unchecked_mut(entity_idx as usize) };

        data.epoch.bump_again(epoch);
        chunk_epoch.bump_again(epoch);
        entity_epoch.bump(epoch);

        let dst = unsafe {
            NonNull::new_unchecked(
                data.ptr
                    .as_ptr()
                    .add((entity_idx as usize) * size_of::<T>()),
            )
        };

        if let Some(encoder) = occupied {
            component.set_one(dst, NonNull::from(&value).cast(), id, encoder)
        } else {
            unsafe {
                ptr::write(dst.as_ptr().cast(), value);
            }
        }
    }

    #[inline(always)]
    unsafe fn relocate_components<F>(
        &mut self,
        src_entity_idx: u32,
        dst: &mut Archetype,
        dst_entity_idx: u32,
        mut missing: F,
    ) where
        F: FnMut(&ComponentInfo, NonNull<u8>),
    {
        let dst_chunk_idx = chunk_idx(dst_entity_idx);

        let last_entity_idx = (self.entities.len() - 1) as u32;

        for (type_id, src_component) in &mut self.components {
            let src_data = src_component.data.get_mut();
            let size = src_component.info.layout().size();
            let src_ptr = unsafe { src_data.ptr.as_ptr().add((src_entity_idx as usize) * size) };

            if let Some(dst_component) = dst.components.get_mut(type_id) {
                let dst_data = dst_component.data.get_mut();

                let epoch = unsafe {
                    *src_data
                        .entity_epochs
                        .get_unchecked(src_entity_idx as usize)
                };
                let dst_chunk_epochs = unsafe {
                    dst_data
                        .chunk_epochs
                        .get_unchecked_mut(dst_chunk_idx as usize)
                };
                let dst_entity_epoch = unsafe {
                    dst_data
                        .entity_epochs
                        .get_unchecked_mut(dst_entity_idx as usize)
                };

                dst_data.epoch.update(epoch);
                dst_chunk_epochs.update(epoch);

                debug_assert_eq!(*dst_entity_epoch, EpochId::start());
                *dst_entity_epoch = epoch;

                let dst_ptr =
                    unsafe { dst_data.ptr.as_ptr().add((dst_entity_idx as usize) * size) };

                unsafe {
                    ptr::copy_nonoverlapping(src_ptr, dst_ptr, size);
                }
            } else {
                let src_ptr = unsafe {
                    NonNull::new_unchecked(
                        src_data.ptr.as_ptr().add((src_entity_idx as usize) * size),
                    )
                };
                missing(&src_component.info, src_ptr);
            }

            if src_entity_idx != last_entity_idx {
                let src_chunk_idx = chunk_idx(src_entity_idx);

                let last_epoch = unsafe {
                    *src_data
                        .entity_epochs
                        .as_ptr()
                        .add(last_entity_idx as usize)
                };
                let src_chunk_epoch = unsafe {
                    src_data
                        .chunk_epochs
                        .get_unchecked_mut(src_chunk_idx as usize)
                };
                let src_entity_epoch = unsafe {
                    src_data
                        .entity_epochs
                        .get_unchecked_mut(src_entity_idx as usize)
                };

                src_chunk_epoch.update(last_epoch);
                *src_entity_epoch = last_epoch;

                let last_ptr =
                    unsafe { src_data.ptr.as_ptr().add((last_entity_idx as usize) * size) };
                unsafe {
                    ptr::copy_nonoverlapping(last_ptr, src_ptr, size);
                }
            }

            #[cfg(debug_assertions)]
            unsafe {
                *src_data
                    .entity_epochs
                    .get_unchecked_mut(last_entity_idx as usize) = EpochId::start();
            }
        }
    }
}

pub(crate) const CHUNK_LEN: u32 = 0x100;

#[inline(always)]
pub(crate) const fn chunk_idx(idx: u32) -> u32 {
    idx >> 8
}

#[inline(always)]
pub(crate) const fn chunks_count(entities: u32) -> u32 {
    entities + (CHUNK_LEN - 1) / CHUNK_LEN
}

#[inline(always)]
pub(crate) const fn first_of_chunk(idx: u32) -> Option<u32> {
    if idx % CHUNK_LEN == 0 {
        Some(chunk_idx(idx))
    } else {
        None
    }
}
