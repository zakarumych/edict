use core::{
    alloc::Layout,
    any::TypeId,
    cell::UnsafeCell,
    hint::unreachable_unchecked,
    intrinsics::copy_nonoverlapping,
    mem::{self, MaybeUninit},
    ops::Deref,
    ptr::{self, NonNull},
};

use alloc::{
    alloc::{alloc, alloc_zeroed, dealloc},
    boxed::Box,
    vec::Vec,
};

use crate::{
    bundle::DynamicBundle,
    component::{Component, ComponentInfo},
    entity::WeakEntity,
    idx::MAX_IDX_USIZE,
    typeidset::TypeIdSet,
};

/// Collection of all entities with same set of components.
/// Archetypes are typically managed by the `World` instance.
///
/// This type is exposed for `Query` implementations.
#[derive(Debug)]
pub struct Archetype {
    set: TypeIdSet,
    indices: Box<[usize]>,
    entities: Vec<WeakEntity>,
    components: Box<[UnsafeCell<ComponentData>]>,
}

#[derive(Debug)]
pub(crate) struct ComponentData {
    pub ptr: NonNull<u8>,
    pub version: u64,
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
            ptr: unsafe { NonNull::new_unchecked(info.layout.align() as _) },
            version: 0,
            chunk_versions: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            info: *info,
        }
    }

    pub fn dummy() -> Self {
        struct Dummy;
        Self::new(&ComponentInfo::of::<Dummy>())
    }

    pub unsafe fn grow(&mut self, old: usize, new: usize) {
        let old_layout = Layout::from_size_align_unchecked(
            self.info.layout.size() * old,
            self.info.layout.align(),
        );

        let new_layout = Layout::from_size_align_unchecked(
            self.info.layout.size() * new,
            self.info.layout.align(),
        );

        if self.info.layout.size() != 0 {
            let mut ptr = NonNull::new_unchecked(alloc(new_layout));
            if old != 0 {
                copy_nonoverlapping(self.ptr.as_ptr(), ptr.as_ptr(), old_layout.size());
                mem::swap(&mut self.ptr, &mut ptr);
                dealloc(ptr.as_ptr(), old_layout);
            } else {
                self.ptr = ptr;
            }
        }

        let mut ptr =
            NonNull::new_unchecked(alloc_zeroed(Layout::array::<u64>(new).unwrap())).cast();
        if old != 0 {
            copy_nonoverlapping(self.entity_versions.as_ptr(), ptr.as_ptr(), old);
            mem::swap(&mut self.entity_versions, &mut ptr);
            dealloc(ptr.cast().as_ptr(), Layout::array::<u64>(old).unwrap());
        } else {
            self.entity_versions = ptr;
        }

        if chunks_count(new) > chunks_count(old) {
            let old = chunks_count(old);
            let new = chunks_count(new);

            let mut ptr =
                NonNull::new_unchecked(alloc_zeroed(Layout::array::<u64>(new).unwrap())).cast();

            if old != 0 {
                copy_nonoverlapping(self.chunk_versions.as_ptr(), ptr.as_ptr(), old);
                mem::swap(&mut self.chunk_versions, &mut ptr);
                dealloc(ptr.cast().as_ptr(), Layout::array::<u64>(old).unwrap());
            } else {
                self.chunk_versions = ptr;
            }
        }
    }
}

impl Archetype {
    /// Creates new archetype with the given set of components.
    pub fn new<'a>(components: impl Iterator<Item = &'a ComponentInfo> + Clone) -> Self {
        let set = TypeIdSet::new(components.clone().map(|c| c.id));

        let mut component_data: Box<[_]> = (0..set.upper_bound())
            .map(|_| UnsafeCell::new(ComponentData::dummy()))
            .collect();

        let indices = set.indexed().map(|(idx, _)| idx).collect();

        for c in components {
            debug_assert_eq!(c.layout.pad_to_align(), c.layout);

            let idx = unsafe { set.get(c.id).unwrap_unchecked() };
            component_data[idx] = UnsafeCell::new(ComponentData::new(c));
        }

        Archetype {
            set,
            indices,
            entities: Vec::new(),
            components: component_data,
        }
    }

    /// Returns `true` if archetype contains compoment with specified id.
    #[inline]
    pub fn contains_id(&self, type_id: TypeId) -> bool {
        self.set.contains_id(type_id)
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    #[inline]
    pub(crate) fn id_index(&self, type_id: TypeId) -> Option<usize> {
        self.set.get(type_id)
    }

    /// Returns `true` if archetype matches compoments set specified.
    #[inline]
    pub fn matches(&self, mut type_ids: impl Iterator<Item = TypeId>) -> bool {
        match type_ids.size_hint() {
            (l, Some(u)) if l == u && l == self.set.len() => {
                type_ids.all(|type_id| self.set.contains_id(type_id))
            }
            _ => false,
        }
    }

    /// Returns iterator over component type ids.
    #[inline]
    pub fn ids(&self) -> impl ExactSizeIterator<Item = TypeId> + Clone + '_ {
        self.indices
            .iter()
            .map(move |&idx| unsafe { (*self.components[idx].get()).id })
    }

    /// Returns iterator over component type infos.
    #[inline]
    pub fn infos(&self) -> impl ExactSizeIterator<Item = &'_ ComponentInfo> + Clone + '_ {
        self.indices
            .iter()
            .map(move |&idx| unsafe { &(*self.components[idx].get()).info })
    }

    /// Spawns new entity in the archetype.
    ///
    /// Returns index of the newly created entity in the archetype.
    pub fn spawn<B>(&mut self, entity: WeakEntity, bundle: B, epoch: u64, reserve: usize) -> u32
    where
        B: DynamicBundle,
    {
        debug_assert!(bundle.with_ids(|ids| self.matches(ids.iter().copied())));
        debug_assert!(self.entities.len() < MAX_IDX_USIZE);

        let entity_idx = self.entities.len();

        unsafe {
            self.reserve(reserve + 1);
            self.write_bundle(entity_idx, bundle, epoch, |_| false);
        }

        self.entities.push(entity);
        entity_idx as u32
    }

    /// Despawns specified entity in the archetype.
    ///
    /// Returns id of the entity that took the place of despawned.
    #[inline]
    pub fn despawn(&mut self, idx: u32) -> Option<u32> {
        assert!(idx < self.entities.len() as u32);

        unsafe { self.despawn_unchecked(idx) }
    }

    /// Despawns specified entity in the archetype.
    ///
    /// Returns id of the entity that took the place of despawned.
    ///
    /// # Safety
    ///
    /// idx must be in bounds of the archetype entities array.
    pub unsafe fn despawn_unchecked(&mut self, idx: u32) -> Option<u32> {
        let entity_idx = idx as usize;
        debug_assert!(entity_idx < self.entities.len());

        let last_entity_idx = self.entities.len() - 1;

        for &type_idx in self.indices.iter() {
            let component = self.components[type_idx].get_mut();
            let size = component.layout.size();

            let ptr = component.ptr.as_ptr().add(entity_idx * size);

            (component.drop_one)(ptr);

            if entity_idx != last_entity_idx {
                let last_ptr = component.ptr.as_ptr().add(last_entity_idx * size);
                ptr::copy_nonoverlapping(last_ptr, ptr, size);
            }
        }

        self.entities.swap_remove(entity_idx);
        if entity_idx != last_entity_idx {
            Some(self.entities[entity_idx].id)
        } else {
            None
        }
    }

    /// Set components from bundle to the entity.
    ///
    /// # Safety
    ///
    /// Bundle must not contain components that are absent in this archetype.
    pub unsafe fn set_bundle<B>(&mut self, idx: u32, bundle: B, epoch: u64)
    where
        B: DynamicBundle,
    {
        let entity_idx = idx as usize;
        debug_assert!(bundle.with_ids(|ids| ids.iter().all(|&id| self.set.get(id).is_some())));
        debug_assert!(entity_idx < self.entities.len());

        self.write_bundle(entity_idx, bundle, epoch, |_| true);
    }

    /// Set component to the entity
    ///
    /// # Safety
    ///
    /// Archetype must contain that component type.
    pub unsafe fn set<T>(&mut self, idx: u32, value: T, epoch: u64)
    where
        T: Component,
    {
        let entity_idx = idx as usize;

        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert!(entity_idx < self.entities.len());

        self.write_one(entity_idx, value, epoch, true);
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
        dst: &mut Archetype,
        src_idx: u32,
        bundle: B,
        epoch: u64,
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
        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |_, _| unsafe {
            unreachable_unchecked()
        });

        dst.write_bundle(dst_entity_idx, bundle, epoch, |id| {
            self.set.get(id).is_some()
        });

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].id),
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
    pub unsafe fn insert<T>(
        &mut self,
        dst: &mut Archetype,
        src_idx: u32,
        value: T,
        epoch: u64,
    ) -> (u32, Option<u32>)
    where
        T: Component,
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

        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |_, _| unsafe {
            unreachable_unchecked()
        });

        dst.write_one::<T>(dst_entity_idx, value, epoch, false);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].id),
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
    pub unsafe fn remove<T>(&mut self, dst: &mut Archetype, src_idx: u32) -> (u32, Option<u32>, T)
    where
        T: Component,
    {
        debug_assert!(dst.ids().all(|id| self.set.get(id).is_some()));
        debug_assert!(dst.set.get(TypeId::of::<T>()).is_none());
        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert_eq!(dst.set.len() + 1, self.set.len());

        let src_entity_idx = src_idx as usize;
        debug_assert!(src_entity_idx < self.entities.len());

        let dst_entity_idx = dst.entities.len();
        debug_assert!(dst_entity_idx < MAX_IDX_USIZE);

        dst.reserve(1);

        let mut value = MaybeUninit::uninit();

        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |info, ptr| unsafe {
            if info.id != TypeId::of::<T>() {
                unreachable_unchecked()
            }
            ptr::copy_nonoverlapping(ptr.cast(), value.as_mut_ptr(), 1)
        });

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].id),
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
    pub unsafe fn drop_bundle(&mut self, dst: &mut Archetype, src_idx: u32) -> (u32, Option<u32>) {
        debug_assert!(dst.ids().all(|id| self.set.get(id).is_some()));

        let src_entity_idx = src_idx as usize;
        debug_assert!(src_entity_idx < self.entities.len());

        let dst_entity_idx = dst.entities.len();
        debug_assert!(dst_entity_idx < MAX_IDX_USIZE);

        dst.reserve(1);
        self.relocate_components(src_entity_idx, dst, dst_entity_idx, |info, ptr| {
            (info.drop_one)(ptr);
        });

        let entity = self.entities.swap_remove(src_entity_idx);
        dst.entities.push(entity);

        if src_entity_idx != self.entities.len() {
            (
                dst_entity_idx as u32,
                Some(self.entities[src_entity_idx].id),
            )
        } else {
            (dst_entity_idx as u32, None)
        }
    }

    #[inline]
    pub(crate) fn entities(&self) -> &[WeakEntity] {
        &self.entities
    }

    /// Returns iterator over component type infos.
    #[inline]
    pub(crate) unsafe fn data(&self, idx: usize) -> &ComponentData {
        &*self.components.get_unchecked(idx).get()
    }

    /// Returns iterator over component type infos.
    #[inline]
    pub(crate) unsafe fn data_mut(&self, idx: usize) -> &mut ComponentData {
        &mut *self.components.get_unchecked(idx).get()
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.entities.len()
    }

    #[inline]
    unsafe fn reserve(&mut self, additional: usize) {
        if self.entities.len() == self.entities.capacity() {
            self.entities.reserve(additional);

            for &idx in &*self.indices {
                let component = self.components[idx].get_mut();
                component.grow(self.entities.len(), self.entities.capacity());
            }
        }
    }

    #[inline]
    unsafe fn write_bundle<B, F>(&mut self, entity_idx: usize, bundle: B, epoch: u64, occupied: F)
    where
        B: DynamicBundle,
        F: Fn(TypeId) -> bool,
    {
        let chunk_idx = chunk_idx(entity_idx);

        bundle.put(|src, id, size| {
            let component = self.components[self.set.get(id).unwrap_unchecked()].get_mut();
            let chunk_version = &mut *component.chunk_versions.as_ptr().add(chunk_idx);
            let entity_version = &mut *component.entity_versions.as_ptr().add(entity_idx);

            debug_assert!(component.version <= epoch);
            component.version = epoch;

            debug_assert!(*chunk_version <= epoch);
            *chunk_version = epoch;

            debug_assert!(*entity_version <= epoch);
            *entity_version = epoch;

            unsafe {
                let dst = component.ptr.as_ptr().add(entity_idx * size);

                if occupied(id) {
                    (component.set_one)(src.as_ptr(), dst);
                } else {
                    ptr::copy_nonoverlapping(src.as_ptr(), dst, size);
                }
            }
        });
    }

    #[inline]
    unsafe fn write_one<T>(&mut self, entity_idx: usize, value: T, epoch: u64, occupied: bool)
    where
        T: Component,
    {
        let chunk_idx = chunk_idx(entity_idx);

        let component =
            self.components[self.set.get(TypeId::of::<T>()).unwrap_unchecked()].get_mut();
        let chunk_version = &mut *component.chunk_versions.as_ptr().add(chunk_idx);
        let entity_version = &mut *component.entity_versions.as_ptr().add(entity_idx);

        debug_assert!(component.version <= epoch);
        component.version = epoch;

        debug_assert!(*chunk_version <= epoch);
        *chunk_version = epoch;

        debug_assert!(*entity_version <= epoch);
        *entity_version = epoch;

        let dst = component.ptr.as_ptr().cast::<T>().add(entity_idx);

        if occupied {
            *dst = value;
        } else {
            ptr::write(dst, value);
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
        F: FnMut(&ComponentInfo, *mut u8),
    {
        let dst_chunk_idx = chunk_idx(dst_entity_idx);

        let last_entity_idx = self.entities.len() - 1;

        for &src_type_idx in self.indices.iter() {
            let src_component = self.components[src_type_idx].get_mut();
            let size = src_component.layout.size();
            let type_id = src_component.id;

            if let Some(dst_type_idx) = dst.set.get(type_id) {
                let dst_component = dst.components[dst_type_idx].get_mut();

                let epoch = *src_component.entity_versions.as_ptr().add(src_entity_idx);

                let dst_chunk_version =
                    &mut *dst_component.chunk_versions.as_ptr().add(dst_chunk_idx);
                let dst_entity_version =
                    &mut *dst_component.entity_versions.as_ptr().add(dst_entity_idx);

                if dst_component.version < epoch {
                    dst_component.version = epoch;
                }

                if *dst_chunk_version < epoch {
                    *dst_chunk_version = epoch;
                }

                debug_assert!(*dst_entity_version <= epoch);
                *dst_entity_version = epoch;

                let src_ptr = src_component.ptr.as_ptr().add(src_entity_idx * size);
                let dst_ptr = dst_component.ptr.as_ptr().add(dst_entity_idx * size);

                ptr::copy_nonoverlapping(src_ptr, dst_ptr, size);

                if src_entity_idx != last_entity_idx {
                    let last_ptr = src_component.ptr.as_ptr().add(last_entity_idx * size);
                    ptr::copy_nonoverlapping(last_ptr, src_ptr, size);
                }
            } else {
                let src_ptr = src_component.ptr.as_ptr().add(src_entity_idx * size);
                missing(src_component, src_ptr);
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
