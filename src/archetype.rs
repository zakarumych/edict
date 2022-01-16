use core::{
    alloc::Layout,
    any::TypeId,
    cell::UnsafeCell,
    ops::Deref,
    ptr::{self, NonNull},
};

use alloc::{alloc::alloc, boxed::Box, vec::Vec};

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
    version: u64,
    info: ComponentInfo,
    chunks: Vec<Chunk>,
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
            version: 0,
            info: *info,
            chunks: Vec::new(),
        }
    }

    pub fn dummy() -> Self {
        struct Dummy;
        Self::new(&ComponentInfo::of::<Dummy>())
    }
}

#[derive(Debug)]
pub(crate) struct Chunk {
    /// Max version of all components in chunk.
    pub version: u64,

    /// Pointer to the beginning of the components in chunk.
    pub ptr: NonNull<u8>,

    pub versions: [u64; CHUNK_LEN_USIZE],
}

impl Chunk {
    pub fn unmodified(&self, since: u64) -> bool {
        since >= self.version
    }
}

impl ComponentData {
    unsafe fn alloc_chunk(&self) -> Chunk {
        let ptr = match self.layout.size() {
            0 => NonNull::new_unchecked(self.layout.align() as *mut u8),
            _ => NonNull::new_unchecked(alloc(Layout::from_size_align_unchecked(
                self.layout.size() * CHUNK_LEN_USIZE,
                CHUNK_ALIGN.max(self.layout.align()),
            ))),
        };

        Chunk {
            version: 0,
            ptr,
            versions: [0; CHUNK_LEN_USIZE],
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

            let idx = set.get(c.id).unwrap();
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
    pub fn contains_id(&self, type_id: TypeId) -> bool {
        self.set.contains_id(type_id)
    }

    /// Returns index of the component type with specified id.
    /// This index may be used then to index into lists of ids and infos.
    pub(crate) fn id_index(&self, type_id: TypeId) -> Option<usize> {
        self.set.get(type_id)
    }

    /// Returns `true` if archetype matches compoments set specified.
    pub fn matches(&self, mut type_ids: impl Iterator<Item = TypeId>) -> bool {
        match type_ids.size_hint() {
            (l, Some(u)) if l == u && l == self.set.len() => {
                type_ids.all(|type_id| self.set.contains_id(type_id))
            }
            _ => false,
        }
    }

    /// Returns iterator over component type ids.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = TypeId> + Clone + '_ {
        self.indices
            .iter()
            .map(move |&idx| unsafe { (*self.components[idx].get()).id })
    }

    /// Returns iterator over component type infos.
    pub fn infos(&self) -> impl ExactSizeIterator<Item = &'_ ComponentInfo> + Clone + '_ {
        self.indices
            .iter()
            .map(move |&idx| unsafe { &(*self.components[idx].get()).info })
    }

    /// Spawns new entity in the archetype.
    ///
    /// Returns index of the newly created entity in the archetype.
    pub fn spawn(&mut self, entity: WeakEntity, bundle: impl DynamicBundle, epoch: u64) -> u32 {
        debug_assert!(bundle.with_ids(|ids| self.matches(ids.iter().copied())));
        debug_assert!(self.entities.len() < MAX_IDX_USIZE);

        let (chunk_idx, entity_idx) = split_idx(self.entities.len() as u32);

        if entity_idx == 0 {
            for &idx in &*self.indices {
                let component = self.components[idx].get_mut();
                debug_assert_eq!(chunk_idx, component.chunks.len());
                component.chunks.push(unsafe { component.alloc_chunk() })
            }
        }

        bundle.put(|src, id, size| {
            let component = self.components[self.set.get(id).unwrap()].get_mut();
            let chunk = &mut component.chunks[chunk_idx];

            debug_assert!(component.version < epoch);
            component.version = epoch;

            debug_assert!(chunk.version < epoch);
            chunk.version = epoch;

            debug_assert!(chunk.versions[entity_idx] < epoch);
            chunk.versions[entity_idx] = epoch;

            unsafe {
                let dst = chunk.ptr.as_ptr().add(entity_idx * size);
                ptr::copy_nonoverlapping(src.as_ptr(), dst, size);
            }
        });

        self.entities.push(entity);
        self.entities.len() as u32 - 1
    }

    /// Despawns specified entity in the archetype.
    ///
    /// Returns id of the entity that took the place of despawned.
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
        debug_assert!(idx < self.entities.len() as u32);

        let last_idx = self.entities.len() as u32 - 1;
        let (last_chunk_idx, last_entity_idx) = split_idx(last_idx);

        let (chunk_idx, entity_idx) = split_idx(idx);

        for &type_idx in self.indices.iter() {
            let component = self.components[type_idx].get_mut();
            let size = component.layout.size();
            let chunk = &component.chunks[chunk_idx];

            let ptr = chunk.ptr.as_ptr().add(entity_idx * size);

            (component.drop_one)(ptr);

            if idx != last_idx {
                let last_chunk = &component.chunks[last_chunk_idx];
                let last_ptr = last_chunk.ptr.as_ptr().add(last_entity_idx * size);

                (component.copy_one)(last_ptr, ptr);
            }
        }

        self.entities.swap_remove(idx as usize);
        if idx != last_idx {
            Some(self.entities[idx as usize].id)
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
        debug_assert!(bundle.with_ids(|ids| ids.iter().all(|&id| self.set.get(id).is_some())));
        debug_assert!(idx < self.entities.len() as u32);

        let (chunk_idx, entity_idx) = split_idx(idx);

        bundle.put(|src, id, size| {
            let component = self.components[self.set.get(id).unwrap()].get_mut();
            let chunk = &mut component.chunks[chunk_idx];

            debug_assert!(component.version < epoch);
            component.version = epoch;

            debug_assert!(chunk.version < epoch);
            chunk.version = epoch;

            debug_assert!(chunk.versions[entity_idx] < epoch);
            chunk.versions[entity_idx] = epoch;

            unsafe {
                let dst = chunk.ptr.as_ptr().add(entity_idx * size);
                (component.set_one)(src.as_ptr(), dst);
            }
        });
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
        debug_assert!(self.set.get(TypeId::of::<T>()).is_some());
        debug_assert!(idx < self.entities.len() as u32);

        let type_id = TypeId::of::<T>();
        let type_idx = self.set.get(type_id).unwrap();

        let (chunk_idx, entity_idx) = split_idx(idx);

        let component = self.components[type_idx].get_mut();
        let chunk = &mut component.chunks[chunk_idx];

        debug_assert!(component.version < epoch);
        component.version = epoch;

        debug_assert!(chunk.version < epoch);
        chunk.version = epoch;

        debug_assert!(chunk.versions[entity_idx] < epoch);
        chunk.versions[entity_idx] = epoch;

        let dst = chunk.ptr.as_ptr().cast::<T>().add(entity_idx);
        *dst = value;
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

        debug_assert!(src_idx < self.entities.len() as u32);
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let dst_idx = dst.entities.len() as u32;
        let (dst_chunk_idx, dst_entity_idx) = split_idx(dst_idx);

        if dst_entity_idx == 0 {
            for &idx in &*dst.indices {
                let component = dst.components[idx].get_mut();
                debug_assert_eq!(dst_chunk_idx, component.chunks.len());
                component.chunks.push(component.alloc_chunk())
            }
        }

        let (src_chunk_idx, src_entity_idx) = split_idx(src_idx);
        let last_idx = self.entities.len() as u32 - 1;
        let (last_chunk_idx, last_entity_idx) = split_idx(last_idx);

        for &src_type_idx in self.indices.iter() {
            let component = self.components[src_type_idx].get_mut();
            let size = component.layout.size();
            let type_id = component.id;
            let src_chunk = &component.chunks[src_chunk_idx];

            let dst_type_idx = dst.set.get(type_id).unwrap();
            let dst_component = dst.components[dst_type_idx].get_mut();
            let dst_chunk = &mut dst_component.chunks[dst_chunk_idx];

            let epoch = src_chunk.versions[src_entity_idx];

            if dst_component.version < epoch {
                dst_component.version = epoch;
            }

            if dst_chunk.version < epoch {
                dst_chunk.version = epoch;
            }

            debug_assert!(dst_chunk.versions[dst_entity_idx] < epoch);
            dst_chunk.versions[dst_entity_idx] = epoch;

            let src_ptr = src_chunk.ptr.as_ptr().add(src_entity_idx * size);
            let dst_ptr = dst_chunk.ptr.as_ptr().add(dst_entity_idx * size);

            (component.copy_one)(src_ptr, dst_ptr);

            if src_idx != last_idx {
                let last_chunk = &component.chunks[last_chunk_idx];
                let last_ptr = last_chunk.ptr.as_ptr().add(last_entity_idx * size);

                (component.copy_one)(last_ptr, src_ptr);
            }
        }

        bundle.put(|src_ptr, id, size| {
            let component = dst.components[dst.set.get(id).unwrap()].get_mut();
            let chunk = &mut component.chunks[dst_chunk_idx];

            debug_assert!(component.version < epoch);
            component.version = epoch;

            debug_assert!(chunk.version < epoch);
            chunk.version = epoch;

            debug_assert!(chunk.versions[dst_entity_idx] < epoch);
            chunk.versions[dst_entity_idx] = epoch;

            unsafe {
                let dst_ptr = chunk.ptr.as_ptr().add(dst_entity_idx * size);

                if self.set.get(id).is_some() {
                    (component.drop_one)(dst_ptr);
                }

                ptr::copy_nonoverlapping(src_ptr.as_ptr(), dst_ptr, size);
            }
        });

        let entity = self.entities.swap_remove(src_idx as usize);
        dst.entities.push(entity);

        if src_idx != last_idx {
            (dst_idx, Some(self.entities[src_idx as usize].id))
        } else {
            (dst_idx, None)
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

        debug_assert!(src_idx < self.entities.len() as u32);
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let type_idx = dst.set.get(TypeId::of::<T>()).unwrap();

        let dst_idx = dst.entities.len() as u32;
        let (dst_chunk_idx, dst_entity_idx) = split_idx(dst_idx);

        if dst_entity_idx == 0 {
            for &idx in &*dst.indices {
                let component = dst.components[idx].get_mut();
                debug_assert_eq!(dst_chunk_idx, component.chunks.len());
                component.chunks.push(component.alloc_chunk())
            }
        }

        let dst_component = dst.components[type_idx].get_mut();
        let dst_chunk = &mut dst_component.chunks[dst_chunk_idx];

        debug_assert!(dst_component.version < epoch);
        dst_component.version = epoch;

        debug_assert!(dst_chunk.version < epoch);
        dst_chunk.version = epoch;

        debug_assert!(dst_chunk.versions[dst_entity_idx] < epoch);
        dst_chunk.versions[dst_entity_idx] = epoch;

        let ptr = dst_chunk.ptr.as_ptr().cast::<T>().add(dst_entity_idx);
        ptr::write(ptr, value);

        let (src_chunk_idx, src_entity_idx) = split_idx(src_idx);
        let last_idx = self.entities.len() as u32 - 1;
        let (last_chunk_idx, last_entity_idx) = split_idx(last_idx);

        for &src_type_idx in self.indices.iter() {
            let src_component = self.components[src_type_idx].get_mut();
            let size = src_component.layout.size();
            let type_id = src_component.id;
            let src_chunk = &src_component.chunks[src_chunk_idx];

            let dst_type_idx = dst.set.get(type_id).unwrap();
            let dst_component = dst.components[dst_type_idx].get_mut();
            let dst_chunk = &mut dst_component.chunks[dst_chunk_idx];

            let epoch = src_chunk.versions[src_entity_idx];

            if dst_component.version < epoch {
                dst_component.version = epoch;
            }

            if dst_chunk.version < epoch {
                dst_chunk.version = epoch;
            }

            debug_assert!(dst_chunk.versions[dst_entity_idx] < epoch);
            dst_chunk.versions[dst_entity_idx] = epoch;

            let src_ptr = src_chunk.ptr.as_ptr().add(src_entity_idx * size);
            let dst_ptr = dst_chunk.ptr.as_ptr().add(dst_entity_idx * size);

            (src_component.copy_one)(src_ptr, dst_ptr);

            if src_idx != last_idx {
                let last_chunk = &src_component.chunks[last_chunk_idx];
                let last_ptr = last_chunk.ptr.as_ptr().add(last_entity_idx * size);

                (src_component.copy_one)(last_ptr, src_ptr);
            }
        }

        let entity = self.entities.swap_remove(src_idx as usize);
        dst.entities.push(entity);

        if src_idx != last_idx {
            (dst_idx, Some(self.entities[src_idx as usize].id))
        } else {
            (dst_idx, None)
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

        debug_assert!(src_idx < self.entities.len() as u32);
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let type_idx = self.set.get(TypeId::of::<T>()).unwrap();

        let dst_idx = dst.entities.len() as u32;
        let (dst_chunk_idx, dst_entity_idx) = split_idx(dst_idx);

        if dst_entity_idx == 0 {
            for &idx in &*dst.indices {
                let component = dst.components[idx].get_mut();
                debug_assert_eq!(dst_chunk_idx, component.chunks.len());
                component.chunks.push(component.alloc_chunk())
            }
        }

        let (src_chunk_idx, src_entity_idx) = split_idx(src_idx);
        let last_idx = self.entities.len() as u32 - 1;
        let (last_chunk_idx, last_entity_idx) = split_idx(last_idx);

        let src_component = self.components[type_idx].get_mut();
        let src_chunk = &mut src_component.chunks[src_chunk_idx];

        let src_ptr = src_chunk.ptr.as_ptr().cast::<T>().add(src_entity_idx);
        let component = ptr::read(src_ptr);

        if src_idx != last_idx {
            let last_chunk = &src_component.chunks[last_chunk_idx];
            let last_ptr = last_chunk.ptr.as_ptr().cast::<T>().add(last_entity_idx);
            ptr::copy_nonoverlapping(last_ptr, src_ptr, 1);
        }

        for &src_type_idx in self.indices.iter() {
            if src_type_idx == type_idx {
                continue;
            }
            let src_component = self.components[src_type_idx].get_mut();
            let size = src_component.layout.size();
            let type_id = src_component.id;

            let src_chunk = &src_component.chunks[src_chunk_idx];

            let dst_type_idx = dst.set.get(type_id).unwrap();
            let dst_component = dst.components[dst_type_idx].get_mut();
            let dst_chunk = &mut dst_component.chunks[dst_chunk_idx];

            let epoch = src_chunk.versions[src_entity_idx];

            if dst_component.version < epoch {
                dst_component.version = epoch;
            }

            if dst_chunk.version < epoch {
                dst_chunk.version = epoch;
            }

            debug_assert!(dst_chunk.versions[dst_entity_idx] < epoch);
            dst_chunk.versions[dst_entity_idx] = epoch;

            let src_ptr = src_chunk.ptr.as_ptr().add(src_entity_idx * size);
            let dst_ptr = dst_chunk.ptr.as_ptr().add(dst_entity_idx * size);

            (src_component.copy_one)(src_ptr, dst_ptr);

            if src_idx != last_idx {
                let last_chunk = &src_component.chunks[last_chunk_idx];
                let last_ptr = last_chunk.ptr.as_ptr().add(last_entity_idx * size);

                (src_component.copy_one)(last_ptr, src_ptr);
            }
        }

        let entity = self.entities.swap_remove(src_idx as usize);
        dst.entities.push(entity);

        if src_idx != last_idx {
            (dst_idx, Some(self.entities[src_idx as usize].id), component)
        } else {
            (dst_idx, None, component)
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

        debug_assert!(src_idx < self.entities.len() as u32);
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let dst_idx = dst.entities.len() as u32;
        let (dst_chunk_idx, dst_entity_idx) = split_idx(dst_idx);

        if dst_entity_idx == 0 {
            for &idx in &*dst.indices {
                let component = dst.components[idx].get_mut();
                debug_assert_eq!(dst_chunk_idx, component.chunks.len());
                component.chunks.push(component.alloc_chunk())
            }
        }

        let (src_chunk_idx, src_entity_idx) = split_idx(src_idx);
        let last_idx = self.entities.len() as u32 - 1;
        let (last_chunk_idx, last_entity_idx) = split_idx(last_idx);

        for &src_type_idx in self.indices.iter() {
            let src_component = self.components[src_type_idx].get_mut();
            let size = src_component.layout.size();
            let type_id = src_component.id;
            let src_chunk = &src_component.chunks[src_chunk_idx];
            let src_ptr = src_chunk.ptr.as_ptr().add(src_entity_idx * size);

            match dst.set.get(type_id) {
                None => {
                    (src_component.drop_one)(src_ptr);
                }
                Some(dst_type_idx) => {
                    let dst_component = dst.components[dst_type_idx].get_mut();
                    let dst_chunk = &mut dst_component.chunks[dst_chunk_idx];

                    let epoch = src_chunk.versions[src_entity_idx];

                    if dst_component.version < epoch {
                        dst_component.version = epoch;
                    }

                    if dst_chunk.version < epoch {
                        dst_chunk.version = epoch;
                    }

                    debug_assert!(dst_chunk.versions[dst_entity_idx] < epoch);
                    dst_chunk.versions[dst_entity_idx] = epoch;

                    let src_ptr = src_chunk.ptr.as_ptr().add(src_entity_idx * size);
                    let dst_ptr = dst_chunk.ptr.as_ptr().add(dst_entity_idx * size);

                    (src_component.copy_one)(src_ptr, dst_ptr);
                }
            }

            if src_idx != last_idx {
                let last_chunk = &src_component.chunks[last_chunk_idx];
                let last_ptr = last_chunk.ptr.as_ptr().add(last_entity_idx * size);

                (src_component.copy_one)(last_ptr, src_ptr);
            }
        }

        let entity = self.entities.swap_remove(src_idx as usize);
        dst.entities.push(entity);

        if src_idx != last_idx {
            (dst_idx, Some(self.entities[src_idx as usize].id))
        } else {
            (dst_idx, None)
        }
    }

    pub(crate) unsafe fn get_chunks(&self, idx: usize) -> &[Chunk] {
        &(*self.components[idx].get()).chunks
    }

    pub(crate) unsafe fn get_chunks_mut(&self, idx: usize) -> &mut [Chunk] {
        &mut (*self.components[idx].get()).chunks
    }

    pub(crate) fn get_entities(&self) -> &[WeakEntity] {
        &self.entities
    }
}

pub(crate) const CHUNK_LEN_USIZE: usize = 0x100;
pub(crate) const CHUNK_ALIGN: usize = 0x100;

pub(crate) const fn split_idx(idx: u32) -> (usize, usize) {
    let chunk_idx = idx >> 8;
    let entity_idx = idx as u8;
    (chunk_idx as usize, entity_idx as usize)
}
