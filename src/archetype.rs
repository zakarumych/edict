use core::{
    alloc::Layout,
    any::TypeId,
    ptr::{copy_nonoverlapping, write, NonNull},
};

use alloc::{alloc::alloc, boxed::Box, vec::Vec};

use crate::{
    bundle::DynamicBundle, component::ComponentInfo, idx::MAX_IDX_USIZE, typeidset::TypeIdSet,
    Component,
};

/// Collection of all entities with same set of components.
#[derive(Debug)]
pub struct Archetype {
    set: TypeIdSet,
    indices: Box<[usize]>,
    entities: Vec<EntityData>,
    components: Box<[ComponentData]>,
}

#[derive(Debug)]
pub(crate) struct ComponentData {
    id: TypeId,
    layout: Layout,
    debug_name: &'static str,
    drop: unsafe fn(*mut u8, usize),
    drop_one: unsafe fn(*mut u8),
    copy: unsafe fn(*const u8, *mut u8, usize),
    copy_one: unsafe fn(*const u8, *mut u8),
    chunks: Vec<Chunk>,
    versions: Vec<Box<[u64; CHUNK_LEN_USIZE]>>,
}

impl ComponentData {
    pub fn new(info: &ComponentInfo) -> Self {
        ComponentData {
            id: info.id,
            layout: info.layout,
            debug_name: info.debug_name,
            drop: info.drop,
            drop_one: info.drop_one,
            copy: info.copy,
            copy_one: info.copy_one,
            chunks: Vec::new(),
            versions: Vec::new(),
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
    pub fn new(components: &[ComponentInfo]) -> Self {
        let set = TypeIdSet::new(components.iter().map(|c| c.id));

        let mut component_data: Box<[_]> = (0..set.upper_bound())
            .map(|_| ComponentData::dummy())
            .collect();

        let indices = set.indexed().map(|(idx, _)| idx).collect();

        for c in components.iter() {
            debug_assert_eq!(c.layout.pad_to_align(), c.layout);

            let idx = set.get(c.id).unwrap();
            component_data[idx] = ComponentData::new(c);
        }

        Archetype {
            set,
            indices,
            entities: Vec::new(),
            components: component_data,
        }
    }

    pub fn contains_id(&self, type_id: TypeId) -> bool {
        self.set.contains_id(type_id)
    }

    pub fn id_index(&self, type_id: TypeId) -> Option<usize> {
        self.set.get(type_id)
    }

    pub fn matches(&self, type_ids: &[TypeId]) -> bool {
        if self.set.len() == type_ids.len() {
            return type_ids
                .iter()
                .all(|&type_id| self.set.contains_id(type_id));
        }
        return false;
    }

    pub fn insert(&mut self, id: u32, bundle: impl DynamicBundle, epoch: u64) -> u32 {
        debug_assert!(self.entities.len() < MAX_IDX_USIZE);
        debug_assert!(bundle.with_ids(|ids| ids.iter().all(|id| self.set.get(*id).is_some())));

        let (chunk_idx, entity_idx) = split_idx(self.entities.len() as u32);

        if entity_idx == 0 {
            for &idx in &*self.indices {
                let component = &mut self.components[idx];
                debug_assert_eq!(chunk_idx, component.chunks.len());
                component.chunks.push(unsafe { component.alloc_chunk() })
            }
        }

        bundle.put(|src, id, size| {
            let component = &mut self.components[self.set.get(id).unwrap()];
            let chunk = &mut component.chunks[chunk_idx];

            chunk.version = epoch;
            chunk.versions[entity_idx] = epoch;

            unsafe {
                let dst = chunk.ptr.as_ptr().add(entity_idx * size);
                copy_nonoverlapping(src.as_ptr(), dst, size);
            }
        });

        self.entities.push(EntityData { id });
        self.entities.len() as u32 - 1
    }

    /// Add component to an entity
    pub fn add_component<T>(
        &mut self,
        dst: &mut Archetype,
        src_idx: u32,
        component: T,
        epoch: u64,
    ) -> u32
    where
        T: Component,
    {
        debug_assert!(self.set.get(TypeId::of::<T>()).is_none());
        debug_assert!(dst.set.get(TypeId::of::<T>()).is_none());
        debug_assert_eq!(self.set.len() + 1, dst.set.len());

        debug_assert!(src_idx < self.entities.len() as u32);
        debug_assert!(dst.entities.len() < MAX_IDX_USIZE);

        let type_idx = dst.set.get(TypeId::of::<T>()).unwrap();

        let dst_idx = dst.indices.len() as u32;
        let (dst_chunk_idx, dst_entity_idx) = split_idx(dst_idx);

        if dst_entity_idx == 0 {
            for &idx in &*dst.indices {
                let component = &mut self.components[idx];
                debug_assert_eq!(dst_chunk_idx, component.chunks.len());
                component.chunks.push(unsafe { component.alloc_chunk() })
            }
        }

        unsafe {
            let chunk = &mut dst.components[type_idx].chunks[dst_chunk_idx];
            chunk.version = epoch;
            chunk.versions[dst_entity_idx] = epoch;
            let ptr = chunk.ptr.as_ptr().cast::<T>().add(dst_entity_idx);
            write(ptr, component);
        }

        let (src_chunk_idx, src_entity_idx) = split_idx(src_idx);
        let last_idx = self.entities.len() as u32 - 1;

        for &src_type_idx in self.indices.iter() {
            let src_component = &self.components[src_type_idx];
            let type_id = src_component.id;
            let src_chunk = &src_component.chunks[src_chunk_idx];

            let dst_type_idx = dst.set.get(type_id).unwrap();
            let dst_chunk = &mut dst.components[dst_type_idx].chunks[dst_chunk_idx];

            dst_chunk.version = epoch;
            dst_chunk.versions[dst_entity_idx] = epoch;

            unsafe {
                let src_ptr = src_chunk
                    .ptr
                    .as_ptr()
                    .add(src_entity_idx * src_component.layout.size());

                let dst_ptr = dst_chunk
                    .ptr
                    .as_ptr()
                    .add(dst_entity_idx * src_component.layout.size());

                (src_component.copy_one)(src_ptr, dst_ptr);

                if src_idx != last_idx {
                    let last_chunk = src_component.chunks.last().unwrap();
                    let last_ptr = last_chunk.ptr.as_ptr().add(last_idx as u8 as usize);

                    (src_component.copy_one)(last_ptr, src_ptr);
                }
            }
        }

        let entity = self.entities.swap_remove(src_idx as usize);
        dst.entities.push(entity);

        dst_idx
    }

    pub(crate) unsafe fn get_chunks(&self, idx: usize) -> &[Chunk] {
        &self.components[idx].chunks
    }

    pub(crate) fn get_entities(&self) -> &[EntityData] {
        &self.entities
    }

    pub(crate) fn get_entities_mut(&mut self) -> &mut [EntityData] {
        &mut self.entities
    }
}

pub(crate) const CHUNK_LEN: u32 = 0x100;
pub(crate) const CHUNK_LEN_USIZE: usize = 0x100;
pub(crate) const CHUNK_ALIGN: usize = 0x100;

pub const fn split_idx(idx: u32) -> (usize, usize) {
    let chunk_idx = idx >> 8;
    let entity_idx = idx as u8;
    (chunk_idx as usize, entity_idx as usize)
}

pub const fn make_idx(chunk_idx: u32, entity_idx: u32) -> u32 {
    (chunk_idx << 8) | entity_idx
}

#[derive(Debug)]
pub(crate) struct EntityData {
    pub id: u32,
}
