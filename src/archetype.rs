use core::{
    alloc::Layout,
    any::TypeId,
    cell::UnsafeCell,
    hint::unreachable_unchecked,
    intrinsics::copy_nonoverlapping,
    mem::{self, size_of, MaybeUninit},
    ops::Deref,
    ptr::{self, NonNull},
};

use alloc::{
    alloc::{alloc, alloc_zeroed, dealloc},
    boxed::Box,
    vec::Vec,
};
use hashbrown::HashMap;

use crate::{
    action::ActionEncoder, bundle::DynamicBundle, component::ComponentInfo, entity::EntityId,
    hash::NoOpHasherBuilder, idx::MAX_IDX_USIZE, typeidset::TypeIdSet,
};

struct Dummy;

pub(crate) struct ComponentData {
    pub ptr: NonNull<u8>,
    pub version: UnsafeCell<u64>,
    pub entity_versions: NonNull<u64>,
    pub chunk_versions: NonNull<u64>,
    pub info: ComponentInfo,
}

impl Deref for ComponentData {
    type Target = ComponentInfo;

    fn deref(&self) -> &ComponentInfo {
        &self.info
    }
}

impl ComponentData {
    pub fn new(info: &ComponentInfo) -> Self {
        ComponentData {
            ptr: NonNull::dangling(),
            version: UnsafeCell::new(0),
            chunk_versions: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            info: info.clone(),
        }
    }

    pub fn dummy() -> Self {
        Self::new(&ComponentInfo::external::<Dummy>())
    }

    pub fn is_dummy(&self) -> bool {
        self.info.id() == TypeId::of::<Dummy>()
    }

    pub unsafe fn drop(&mut self, cap: usize, len: usize) {
        if self.is_dummy() {
            return;
        }

        self.info.final_drop(self.ptr, len);

        if self.info.layout().size() != 0 {
            let layout = Layout::from_size_align_unchecked(
                self.info.layout().size() * cap,
                self.info.layout().align(),
            );

            dealloc(self.ptr.as_ptr(), layout);
        }

        dealloc(
            self.entity_versions.cast().as_ptr(),
            Layout::array::<u64>(cap).unwrap(),
        );
        dealloc(
            self.chunk_versions.cast().as_ptr(),
            Layout::array::<u64>(chunks_count(cap)).unwrap(),
        );
    }

    pub unsafe fn grow(&mut self, len: usize, old_cap: usize, new_cap: usize) {
        if self.info.layout().size() != 0 {
            let new_layout = Layout::from_size_align(
                self.info.layout().size().checked_mul(new_cap).unwrap(),
                self.info.layout().align(),
            )
            .unwrap();

            let mut ptr = NonNull::new_unchecked(alloc(new_layout));
            if len != 0 {
                copy_nonoverlapping(
                    self.ptr.as_ptr(),
                    ptr.as_ptr(),
                    len * self.info.layout().size(),
                );
            }

            if old_cap != 0 {
                let old_layout = Layout::from_size_align_unchecked(
                    self.info.layout().size() * old_cap,
                    self.info.layout().align(),
                );

                mem::swap(&mut self.ptr, &mut ptr);
                dealloc(ptr.as_ptr(), old_layout);
            } else {
                self.ptr = ptr;
            }
        }

        let mut ptr =
            NonNull::new_unchecked(alloc_zeroed(Layout::array::<u64>(new_cap).unwrap())).cast();
        if len != 0 {
            copy_nonoverlapping(self.entity_versions.as_ptr(), ptr.as_ptr(), len);
        }

        if old_cap != 0 {
            mem::swap(&mut self.entity_versions, &mut ptr);
            dealloc(ptr.cast().as_ptr(), Layout::array::<u64>(old_cap).unwrap());
        } else {
            self.entity_versions = ptr;
        }

        if chunks_count(new_cap) > chunks_count(old_cap) {
            let old_cap = chunks_count(old_cap);
            let new_cap = chunks_count(new_cap);

            let mut ptr =
                NonNull::new_unchecked(alloc_zeroed(Layout::array::<u64>(new_cap).unwrap())).cast();

            copy_nonoverlapping(self.chunk_versions.as_ptr(), ptr.as_ptr(), len);

            if old_cap != 0 {
                mem::swap(&mut self.chunk_versions, &mut ptr);
                dealloc(ptr.cast().as_ptr(), Layout::array::<u64>(old_cap).unwrap());
            } else {
                self.chunk_versions = ptr;
            }
        }
    }
}

/// Collection of all entities with same set of components.
/// Archetypes are typically managed by the `World` instance.
///
/// This type is exposed for `Query` implementations.
#[allow(missing_debug_implementations)]
pub struct Archetype {
    set: TypeIdSet,
    indices: Box<[usize]>,
    entities: Vec<EntityId>,
    components: Box<[ComponentData]>,
    borrows: HashMap<TypeId, Vec<(usize, usize)>, NoOpHasherBuilder>,
    borrows_mut: HashMap<TypeId, Vec<(usize, usize)>, NoOpHasherBuilder>,
}

impl Drop for Archetype {
    fn drop(&mut self) {
        for c in &mut *self.components {
            unsafe {
                c.drop(self.entities.capacity(), self.entities.len());
            }
        }
    }
}

impl Archetype {
    /// Creates new archetype with the given set of components.
    pub fn new<'a>(components: impl Iterator<Item = &'a ComponentInfo> + Clone) -> Self {
        let set = TypeIdSet::new(components.clone().map(|c| c.id()));

        let mut component_data: Box<[_]> = (0..set.upper_bound())
            .map(|_| ComponentData::dummy())
            .collect();

        let indices = set.indexed().map(|(idx, _)| idx).collect();

        for c in components.clone() {
            debug_assert_eq!(c.layout().pad_to_align(), c.layout());

            let idx = unsafe { set.get(c.id()).unwrap_unchecked() };
            component_data[idx] = ComponentData::new(c);
        }

        let mut borrows = HashMap::with_hasher(NoOpHasherBuilder);
        let mut borrows_mut = HashMap::with_hasher(NoOpHasherBuilder);

        for c in components {
            let cidx = unsafe { set.get(c.id()).unwrap_unchecked() };

            for (bidx, cb) in c.borrows().iter().enumerate() {
                borrows
                    .entry(cb.target())
                    .or_insert_with(Vec::new)
                    .push((cidx, bidx));

                if cb.has_borrow_mut() {
                    borrows_mut
                        .entry(cb.target())
                        .or_insert_with(Vec::new)
                        .push((cidx, bidx));
                }
            }
        }

        Archetype {
            set,
            indices,
            entities: Vec::new(),
            components: component_data,
            borrows,
            borrows_mut,
        }
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline]
    pub fn contains_id(&self, type_id: TypeId) -> bool {
        self.set.contains_id(type_id)
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline]
    pub fn contains_borrow(&self, type_id: TypeId) -> bool {
        self.borrows.contains_key(&type_id)
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline]
    pub fn contains_borrow_mut(&self, type_id: TypeId) -> bool {
        self.borrows_mut.contains_key(&type_id)
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    #[inline]
    pub(crate) fn id_index(&self, type_id: TypeId) -> Option<usize> {
        self.set.get(type_id)
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    #[inline]
    pub(crate) fn borrow_indices(&self, type_id: TypeId) -> Option<&[(usize, usize)]> {
        self.borrows.get(&type_id).map(|v| &v[..])
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    #[inline]
    pub(crate) fn borrow_mut_indices(&self, type_id: TypeId) -> Option<&[(usize, usize)]> {
        self.borrows_mut.get(&type_id).map(|v| &v[..])
    }

    /// Returns `true` if archetype matches components set specified.
    #[inline]
    pub fn matches(&self, mut type_ids: impl Iterator<Item = TypeId>) -> bool {
        match type_ids.size_hint() {
            (l, None) if l <= self.set.len() => {
                type_ids.try_fold(0usize, |count, type_id| {
                    if self.set.contains_id(type_id) {
                        Some(count + 1)
                    } else {
                        None
                    }
                }) == Some(self.set.len())
            }
            (l, Some(u)) if l <= self.set.len() && u >= self.set.len() => {
                type_ids.try_fold(0usize, |count, type_id| {
                    if self.set.contains_id(type_id) {
                        Some(count + 1)
                    } else {
                        None
                    }
                }) == Some(self.set.len())
            }
            _ => false,
        }
    }

    /// Returns iterator over component type ids.
    #[inline]
    pub fn ids(&self) -> impl ExactSizeIterator<Item = TypeId> + Clone + '_ {
        self.indices
            .iter()
            .map(move |&idx| self.components[idx].id())
    }

    /// Returns iterator over component type infos.
    #[inline]
    pub fn infos(&self) -> impl ExactSizeIterator<Item = &'_ ComponentInfo> + Clone + '_ {
        self.indices
            .iter()
            .map(move |&idx| &self.components[idx].info)
    }

    /// Spawns new entity in the archetype.
    ///
    /// Returns index of the newly created entity in the archetype.
    pub fn spawn<B>(&mut self, entity: EntityId, bundle: B, epoch: u64) -> u32
    where
        B: DynamicBundle,
    {
        debug_assert!(bundle.with_ids(|ids| self.matches(ids.iter().copied())));
        debug_assert!(self.entities.len() < MAX_IDX_USIZE);

        let entity_idx = self.entities.len();

        unsafe {
            self.reserve(1);

            debug_assert_ne!(self.entities.len(), self.entities.capacity());
            self.write_bundle(entity, entity_idx, bundle, epoch, None, |_| false);
        }

        self.entities.push(entity);
        entity_idx as u32
    }

    /// Despawns specified entity in the archetype.
    ///
    /// Returns id of the entity that took the place of despawned.
    #[inline]
    pub fn despawn(
        &mut self,
        entity: EntityId,
        idx: u32,
        encoder: &mut ActionEncoder,
    ) -> Option<u32> {
        assert!(idx < self.entities.len() as u32);

        unsafe { self.despawn_unchecked(entity, idx, encoder) }
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
        entity: EntityId,
        idx: u32,
        encoder: &mut ActionEncoder,
    ) -> Option<u32> {
        let entity_idx = idx as usize;
        debug_assert!(entity_idx < self.entities.len());
        debug_assert_eq!(entity, self.entities[entity_idx]);

        let last_entity_idx = self.entities.len() - 1;

        for &type_idx in self.indices.iter() {
            let component = &self.components[type_idx];
            let size = component.layout().size();

            let ptr = NonNull::new_unchecked(component.ptr.as_ptr().add(entity_idx * size));

            component.drop_one(ptr, entity, encoder);

            if entity_idx != last_entity_idx {
                let chunk_idx = chunk_idx(entity_idx);

                let last_epoch = *component.entity_versions.as_ptr().add(last_entity_idx);

                let chunk_version = &mut *component.chunk_versions.as_ptr().add(chunk_idx);
                let entity_version = &mut *component.entity_versions.as_ptr().add(entity_idx);

                if *chunk_version < last_epoch {
                    *chunk_version = last_epoch;
                }

                *entity_version = last_epoch;

                let last_ptr = component.ptr.as_ptr().add(last_entity_idx * size);
                ptr::copy_nonoverlapping(last_ptr, ptr.as_ptr(), size);
            }

            #[cfg(debug_assertions)]
            {
                *component.entity_versions.as_ptr().add(last_entity_idx) = 0;
            }
        }

        self.entities.swap_remove(entity_idx);
        if entity_idx != last_entity_idx {
            Some(self.entities[entity_idx].idx())
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
        entity: EntityId,
        idx: u32,
        bundle: B,
        epoch: u64,
        encoder: &mut ActionEncoder,
    ) where
        B: DynamicBundle,
    {
        let entity_idx = idx as usize;
        debug_assert!(bundle.with_ids(|ids| ids.iter().all(|&id| self.set.get(id).is_some())));
        debug_assert!(entity_idx < self.entities.len());

        self.write_bundle(entity, entity_idx, bundle, epoch, Some(encoder), |_| true);
    }

    /// Set component to the entity
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    #[inline]
    pub unsafe fn set<T>(
        &mut self,
        entity: EntityId,
        idx: u32,
        value: T,
        epoch: u64,
        encoder: &mut ActionEncoder,
    ) where
        T: 'static,
    {
        let entity_idx = idx as usize;

        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert!(entity_idx < self.entities.len());

        self.write_one(entity, entity_idx, value, epoch, Some(encoder));
    }

    /// Get component of the entity
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    #[inline]
    pub unsafe fn get<T>(&mut self, idx: u32) -> &T
    where
        T: 'static,
    {
        let entity_idx = idx as usize;

        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert!(entity_idx < self.entities.len());

        let id = self.set.get_unchecked(TypeId::of::<T>());
        let component = &self.components[id];
        let ptr = component.ptr.as_ptr().cast::<T>().add(entity_idx);
        &*ptr
    }

    /// Borrows component mutably. Updates entity version.
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    #[inline]
    pub unsafe fn get_mut<T>(&mut self, idx: u32, epoch: u64) -> &mut T
    where
        T: 'static,
    {
        let entity_idx = idx as usize;
        let chunk_idx = chunk_idx(entity_idx);

        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert!(entity_idx < self.entities.len());

        let id = self.set.get_unchecked(TypeId::of::<T>());
        let component = &self.components[id];
        let ptr = component.ptr.as_ptr().cast::<T>().add(entity_idx);

        let chunk_version = &mut *component.chunk_versions.as_ptr().add(chunk_idx);
        let entity_version = &mut *component.entity_versions.as_ptr().add(entity_idx);

        debug_assert!(*component.version.get() <= epoch);
        *component.version.get() = epoch;

        debug_assert!(*chunk_version <= epoch);
        *chunk_version = epoch;

        debug_assert!(*entity_version <= epoch);
        *entity_version = epoch;

        &mut *ptr
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
        entity: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
        bundle: B,
        epoch: u64,
        encoder: &mut ActionEncoder,
    ) -> (u32, Option<u32>)
    where
        B: DynamicBundle,
    {
        debug_assert!(self.ids().all(|id| dst.set.get(id).is_some()));
        debug_assert!(bundle.with_ids(|ids| ids.iter().all(|&id| dst.set.get(id).is_some())));

        debug_assert_eq!(
            bundle.with_ids(|ids| { ids.iter().filter(|&id| self.set.get(*id).is_none()).count() })
                + self.set.len(),
            dst.set.len()
        );

        let src_entity_idx = src_idx as usize;

        debug_assert!(src_entity_idx < self.entities.len());
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let dst_entity_idx = dst.entities.len();

        dst.reserve(1);

        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());
        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |_, _| {
            unreachable_unchecked()
        });

        dst.write_bundle(entity, dst_entity_idx, bundle, epoch, Some(encoder), |id| {
            if self.set.get(id).is_some() {
                true
            } else {
                false
            }
        });

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].idx()),
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
        entity: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
        value: T,
        epoch: u64,
    ) -> (u32, Option<u32>)
    where
        T: 'static,
    {
        debug_assert!(self.ids().all(|id| dst.set.get(id).is_some()));
        debug_assert!(self.set.get(TypeId::of::<T>()).is_none());
        debug_assert!(dst.set.get(TypeId::of::<T>()).is_some());
        debug_assert_eq!(self.set.len() + 1, dst.set.len());

        let src_entity_idx = src_idx as usize;
        debug_assert!(src_entity_idx < self.entities.len());

        let dst_entity_idx = dst.entities.len();
        debug_assert!(dst_entity_idx < MAX_IDX_USIZE);

        dst.reserve(1);

        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());
        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |_, _| {
            unreachable_unchecked()
        });

        dst.write_one::<T>(entity, dst_entity_idx, value, epoch, None);

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].idx()),
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
        entity: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
    ) -> (u32, Option<u32>, T)
    where
        T: 'static,
    {
        debug_assert!(dst.ids().all(|id| self.set.get(id).is_some()));
        debug_assert!(dst.set.get(TypeId::of::<T>()).is_none());
        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert_eq!(dst.set.len() + 1, self.set.len());

        let src_entity_idx = src_idx as usize;
        debug_assert!(src_entity_idx < self.entities.len());
        debug_assert_eq!(entity, self.entities[src_entity_idx]);

        let dst_entity_idx = dst.entities.len();
        debug_assert!(dst_entity_idx < MAX_IDX_USIZE);

        let mut value = MaybeUninit::uninit();

        dst.reserve(1);

        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());
        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |info, ptr| {
            if info.id() != TypeId::of::<T>() {
                unreachable_unchecked()
            }

            value.write(ptr::read(ptr.as_ptr().cast()));
        });

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].idx()),
                value.assume_init(),
            )
        } else {
            (dst_entity_idx as u32, None, value.assume_init())
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
        entity: EntityId,
        dst: &mut Archetype,
        src_idx: u32,
        encoder: &mut ActionEncoder,
    ) -> (u32, Option<u32>) {
        debug_assert!(dst.ids().all(|id| self.set.get(id).is_some()));

        let src_entity_idx = src_idx as usize;
        debug_assert!(src_entity_idx < self.entities.len());
        debug_assert_eq!(entity, self.entities[src_entity_idx]);

        let dst_entity_idx = dst.entities.len();
        debug_assert!(dst_entity_idx < MAX_IDX_USIZE);

        dst.reserve(1);
        debug_assert_ne!(dst.entities.len(), dst.entities.capacity());

        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |info, ptr| {
            info.drop_one(ptr, entity, encoder);
        });

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].idx()),
            )
        } else {
            (dst_entity_idx as u32, None)
        }
    }

    #[inline]
    pub(crate) fn entities(&self) -> &[EntityId] {
        &self.entities
    }

    /// Returns iterator over component type infos.
    #[inline]
    pub(crate) unsafe fn data(&self, idx: usize) -> &ComponentData {
        debug_assert!(idx < self.components.len());
        &self.components.get_unchecked(idx)
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.entities.len()
    }

    #[inline]
    pub(crate) fn reserve(&mut self, additional: usize) {
        let old_cap = self.entities.capacity();
        let len = self.entities.len();

        if additional <= old_cap - len {
            return;
        }

        // Needs to grow.

        self.entities.reserve(additional);
        debug_assert_ne!(old_cap, self.entities.capacity(),);

        for &idx in &*self.indices {
            let component = &mut self.components[idx];
            unsafe {
                component.grow(len, old_cap, self.entities.capacity());
            }
        }
    }

    #[inline]
    unsafe fn write_bundle<B, F>(
        &mut self,
        entity: EntityId,
        entity_idx: usize,
        bundle: B,
        epoch: u64,
        mut encoder: Option<&mut ActionEncoder>,
        occupied: F,
    ) where
        B: DynamicBundle,
        F: Fn(TypeId) -> bool,
    {
        let chunk_idx = chunk_idx(entity_idx);

        bundle.put(|src, id, size| {
            let component = &self.components[self.set.get(id).unwrap_unchecked()];
            let chunk_version = &mut *component.chunk_versions.as_ptr().add(chunk_idx);
            let entity_version = &mut *component.entity_versions.as_ptr().add(entity_idx);

            debug_assert!(*component.version.get() <= epoch);
            *component.version.get() = epoch;

            debug_assert!(*chunk_version <= epoch);
            *chunk_version = epoch;

            debug_assert!(*entity_version <= epoch);
            *entity_version = epoch;

            let dst = NonNull::new_unchecked(component.ptr.as_ptr().add(entity_idx * size));
            if occupied(id) {
                component.set_one(dst, src, entity, encoder.as_mut().unwrap());
            } else {
                ptr::copy_nonoverlapping(src.as_ptr(), dst.as_ptr(), size);
            }
        });
    }

    #[inline]
    unsafe fn write_one<T>(
        &mut self,
        entity: EntityId,
        entity_idx: usize,
        value: T,
        epoch: u64,
        occupied: Option<&mut ActionEncoder>,
    ) where
        T: 'static,
    {
        let chunk_idx = chunk_idx(entity_idx);

        let component = &self.components[self.set.get(TypeId::of::<T>()).unwrap_unchecked()];
        let chunk_version = &mut *component.chunk_versions.as_ptr().add(chunk_idx);
        let entity_version = &mut *component.entity_versions.as_ptr().add(entity_idx);

        debug_assert!(*component.version.get() <= epoch);
        *component.version.get() = epoch;

        debug_assert!(*chunk_version <= epoch);
        *chunk_version = epoch;

        debug_assert!(*entity_version <= epoch);
        *entity_version = epoch;

        let dst = NonNull::new_unchecked(component.ptr.as_ptr().add(entity_idx * size_of::<T>()));

        if let Some(encoder) = occupied {
            component.set_one(dst, NonNull::from(&value).cast(), entity, encoder)
        } else {
            ptr::write(dst.as_ptr().cast(), value);
        }
    }

    #[inline]
    unsafe fn relocate_components<F>(
        &mut self,
        src_entity_idx: usize,
        dst: &mut Archetype,
        dst_entity_idx: usize,
        mut missing: F,
    ) where
        F: FnMut(&ComponentInfo, NonNull<u8>),
    {
        let dst_chunk_idx = chunk_idx(dst_entity_idx);

        let last_entity_idx = self.entities.len() - 1;

        for &src_type_idx in self.indices.iter() {
            let src_component = &self.components[src_type_idx];
            let size = src_component.layout().size();
            let type_id = src_component.id();
            let src_ptr = src_component.ptr.as_ptr().add(src_entity_idx * size);

            if let Some(dst_type_idx) = dst.set.get(type_id) {
                let dst_component = &dst.components[dst_type_idx];

                let epoch = *src_component.entity_versions.as_ptr().add(src_entity_idx);

                let dst_chunk_version =
                    &mut *dst_component.chunk_versions.as_ptr().add(dst_chunk_idx);

                let dst_entity_version =
                    &mut *dst_component.entity_versions.as_ptr().add(dst_entity_idx);

                if *dst_component.version.get() < epoch {
                    *dst_component.version.get() = epoch;
                }

                if *dst_chunk_version < epoch {
                    *dst_chunk_version = epoch;
                }

                debug_assert_eq!(*dst_entity_version, 0);
                *dst_entity_version = epoch;

                let dst_ptr = dst_component.ptr.as_ptr().add(dst_entity_idx * size);

                ptr::copy_nonoverlapping(src_ptr, dst_ptr, size);
            } else {
                let src_ptr = src_component.ptr.as_ptr().add(src_entity_idx * size);
                missing(src_component, NonNull::new_unchecked(src_ptr));
            }

            if src_entity_idx != last_entity_idx {
                let src_chunk_idx = chunk_idx(src_entity_idx);

                let last_epoch = *src_component.entity_versions.as_ptr().add(last_entity_idx);

                let src_chunk_version =
                    &mut *src_component.chunk_versions.as_ptr().add(src_chunk_idx);

                let src_entity_version =
                    &mut *src_component.entity_versions.as_ptr().add(src_entity_idx);

                if *src_chunk_version < last_epoch {
                    *src_chunk_version = last_epoch;
                }

                *src_entity_version = last_epoch;

                let last_ptr = src_component.ptr.as_ptr().add(last_entity_idx * size);
                ptr::copy_nonoverlapping(last_ptr, src_ptr, size);
            }
            #[cfg(debug_assertions)]
            {
                *src_component.entity_versions.as_ptr().add(last_entity_idx) = 0;
            }
        }
    }
}

pub(crate) const CHUNK_LEN_USIZE: usize = 0x100;

#[inline]
pub(crate) const fn chunk_idx(idx: usize) -> usize {
    idx >> 8
}

#[inline]
pub(crate) const fn chunks_count(entities: usize) -> usize {
    entities + (CHUNK_LEN_USIZE - 1) / CHUNK_LEN_USIZE
}

#[inline]
pub(crate) const fn first_of_chunk(idx: usize) -> Option<usize> {
    if idx % CHUNK_LEN_USIZE == 0 {
        Some(chunk_idx(idx))
    } else {
        None
    }
}
