//! [`Relation`] is like [`Component`] but they are bound to a pair of entities.
//!
//! [`Component`]: crate::component::Component

use crate::{action::LocalActionEncoder, entity::EntityId};

pub use edict_proc::Relation;

pub use self::{
    child_of::ChildOf,
    query::{
        FetchFilterRelatedBy, FetchRelatedRead, FetchRelatedWith, FetchRelatedWrite,
        FetchRelatesExclusiveRead, FetchRelatesExclusiveWith, FetchRelatesExclusiveWrite,
        FetchRelatesRead, FetchRelatesToRead, FetchRelatesToWrite, FetchRelatesWith,
        FetchRelatesWrite, FilterFetchRelatesTo, FilterRelated, FilterRelatedBy, FilterRelates,
        FilterRelatesTo, Related, Relates, RelatesExclusive, RelatesTo, RelationIter,
        RelationReadIter, RelationWriteIter,
    },
};

pub(crate) use self::components::{OriginComponent, TargetComponent};

mod child_of;
mod components;
mod query;

/// Trait that must be implemented for types to be
/// used as relation components.
///
/// Relation components are special in a way that they are bound to
/// a pair of entities, not just one.
/// One entity is called "origin" and the other is called "target".
///
/// Relation components are used to connect two entities together.
/// For example [`ChildOf`] relation component is used to connect
/// child entity ("origin") to parent entity ("target").
///
/// Relation components are dropped when either of the "origin" or "target"
/// is dropped. Appropriate hook method is called when this happens.
/// `on_drop` is called when relation is dropped from "origin" entity.
/// `on_target_drop` is called when "target" entity is dropped.
pub trait Relation: Copy + 'static {
    /// If `true` then relation can be added only once to an entity.
    /// If another exclusive relation is added to the same entity,
    /// then the old one is removed.
    /// `on_replace` is called when this happens.
    ///
    /// Non-exclusive relations is replaced only if re-added
    /// with same target.
    ///
    /// When using `#[derive(Relation)]` add `#[edict(exclusive)]` attribute to set this to true.
    const EXCLUSIVE: bool = false;

    /// If `true` then when relation is added to an entity
    /// it is also added to the target in reverse direction.
    ///
    /// When using `#[derive(Relation)]` add `#[edict(symmetric)]` attribute to set this to true.
    const SYMMETRIC: bool = false;

    /// If `true` then origin entity in relation is "owned" by the target.
    /// This means that when last target is dropped, entity is despawned.
    ///
    /// When using `#[derive(Relation)]` add `#[edict(owned)]` attribute to set this to true.
    const OWNED: bool = false;

    /// Returns name of the relation type.
    ///
    /// Can be overridden to provide custom name.
    #[inline]
    #[must_use]
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }

    /// Method that is called when relation is re-inserted.
    /// For non-exclusive relations this happens when relation is re-inserted with the same
    /// origin-target entity pair.
    /// For exclusive relations this happens when relation is re-inserted with
    /// origin that has relation of this type with any target.
    ///
    /// If returns `true`, `on_drop` will be called.
    ///
    /// Does nothing by default and returns `true`, causing `on_drop` to be called.
    #[inline]
    fn on_replace(
        old_value: &mut Self,
        new_value: &Self,
        origin: EntityId,
        old_target: EntityId,
        new_target: EntityId,
        encoder: LocalActionEncoder,
    ) -> bool {
        let _ = old_value;
        let _ = new_value;
        let _ = origin;
        let _ = old_target;
        let _ = new_target;
        let _ = encoder;

        true
    }

    /// Hook that is called when relation is dropped
    /// via [`World::drop_relation`](crate::world::World::drop_relation) or similar method
    /// or is replaced and [`Relation::on_replace`] returns `true`.
    #[inline]
    fn on_drop(self, origin: EntityId, target: EntityId, encoder: LocalActionEncoder) {
        let _ = origin;
        let _ = target;
        let _ = encoder;
    }

    /// Hook that is called when origin is despawned.
    #[inline]
    fn on_origin_drop(origin: EntityId, targets: &[(EntityId, Self)], encoder: LocalActionEncoder) {
        let _ = origin;
        let _ = targets;
        let _ = encoder;
    }

    /// Hook that is called when target is despawned.
    #[inline]
    fn on_target_drop(origins: &[(EntityId, Self)], target: EntityId, encoder: LocalActionEncoder) {
        let _ = origins;
        let _ = target;
        let _ = encoder;
    }
}

/// Sub-trait for exclusive relations.
/// It should be implemented for relations that specify `EXCLUSIVE = true`,
/// to enable use of `RelatesExclusive` query.
/// Implementing it for relation with `EXCLUSIVE = false` will cause
/// compilation error or runtime panic.
///
/// `Relation` derive macro implements this trait automatically.
pub trait ExclusiveRelation: Relation {
    #[doc(hidden)]
    const ASSERT_EXCLUSIVE: () = assert!(Self::EXCLUSIVE);
}
