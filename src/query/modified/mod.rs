mod alt;
// mod any_of;
mod copied;
mod read;
mod with;
mod write;

use crate::epoch::EpochId;

pub use self::{
    alt::ModifiedFetchAlt, copied::ModifiedFetchCopied, read::ModifiedFetchRead,
    with::ModifiedFetchWith, write::ModifiedFetchWrite,
};

/// Query over modified component.
///
/// Should be used as either [`Modified<&T>`], [`Modified<&mut T>`]
/// or [`Modified<Alt<T>>`].
///
/// This is tracking query that uses epoch lower bound to filter out entities with unmodified components.
#[derive(Clone, Copy, Debug)]
pub struct Modified<T> {
    after_epoch: EpochId,
    query: T,
}

impl<T> Modified<T> {
    /// Creates new `Modified` query.
    /// Uses provided `after_epoch` id to skip components that are last modified not after this epoch.
    pub fn new(after_epoch: EpochId) -> Self
    where
        T: Default,
    {
        Modified {
            after_epoch,
            query: T::default(),
        }
    }

    /// Epoch id threshold for this query.
    pub fn after_epoch(&self) -> EpochId {
        self.after_epoch
    }
}

// /// Query that concerns exactly one specific component.
// pub trait ComponentQuery: Query {
//     /// Returns archetype component for this query.
//     fn archetype_component(&self, archetype: &Archetype) -> &ArchetypeComponent;
// }

// /// Query that concerns exactly one specific component.
// pub trait ComponentFetch<'a>: Fetch<'a> {
//     /// Returns epoch id of the specified chunk
//     fn chunk_epoch(&self, chunk_idx: u32) -> EpochId;

//     /// Returns epoch id of the specified item.
//     fn item_epoch(&self, idx: u32) -> EpochId;
// }

// pub struct ModifiedFetch<'a, F> {
//     fetch: F,
//     after_epoch: EpochId,
//     epoch: EpochId,
//     entity_epochs: NonNull<EpochId>,
//     chunk_epochs: NonNull<Cell<EpochId>>,
//     archetype_epoch: NonNull<Cell<EpochId>>,
// }

// unsafe impl<'a, F> Fetch<'a> for ModifiedFetch<'a, F>
// where
//     F: Fetch<'a>,
// {
//     type Item = F::Item;

//     fn dangling() -> Self {}
// }

// unsafe impl<Q> Query for Modified<Q>
// where
//     Q: ComponentQuery,
// {
//     type Item<'a> = Q::Item<'a>;
//     type Fetch<'a> = Q::Fetch<'a>;

//     const MUTABLE: bool = true;

//     #[inline(always)]
//     fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
//         self.query.component_type_access(ty)
//     }

//     #[inline(always)]
//     fn visit_archetype(&self, archetype: &Archetype) -> bool {
//         self.query.visit_archetype(archetype)
//     }

//     #[inline(always)]
//     unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
//         self.query.access_archetype(archetype, f);
//     }

//     #[inline(always)]
//     fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
//         if !self.query.visit_archetype_late(archetype) {
//             return false;
//         }

//         let component = self.query.archetype_component(archetype);
//         unsafe {
//             let data = component.data();
//             data.epoch.after(self.after_epoch)
//         }
//     }

//     #[inline(always)]
//     unsafe fn fetch<'a>(
//         &self,
//         _arch_idx: u32,
//         archetype: &'a Archetype,
//         epoch: EpochId,
//     ) -> Option<ModifiedFetch<'a, T>> {
//         match archetype.component(TypeId::of::<T>()) {
//             None => None,
//             Some(component) => {
//                 let data = component.data_mut();

//                 debug_assert!(data.epoch.after(self.after_epoch));

//                 Some(ModifiedFetchAlt {
//                     after_epoch: self.after_epoch,
//                     epoch,
//                     ptr: data.ptr.cast(),
//                     entity_epochs: NonNull::new_unchecked(
//                         data.entity_epochs.as_ptr() as *mut EpochId
//                     ),
//                     chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()).cast(),
//                     archetype_epoch: NonNull::from(&mut data.epoch).cast(),
//                     marker: PhantomData,
//                 })
//             }
//         }
//     }
// }
