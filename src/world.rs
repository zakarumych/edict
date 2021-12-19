use crate::{
    component::Component,
    entity::{Entity, WeakEntity},
    proof::Proof,
    query::Query,
};

/// Entities container.
pub struct World {}

impl World {
    /// Queries components from specified entity.
    ///
    /// Requires access to all components in query.
    /// All components guaranteed by entity reference must be queries or skipped.
    /// Other components are fetched optionally.
    pub fn query_one<'a, Q, A: 'a>(&self, entity: &'a Entity<A>) -> Q
    where
        &'a A: Proof<Q>,
        Q: Query,
    {
        todo!()
    }

    /// Queries components from specified entity.
    ///
    /// Requires access to all components in query.
    /// All components guaranteed by entity reference must be queries or skipped.
    /// Other components are fetched optionally.
    pub fn query_one_mut<'a, Q, A: 'a>(&self, entity: &'a mut Entity<A>) -> Q
    where
        &'a mut A: Proof<Q>,
        Q: Query,
    {
        todo!()
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `WeakError::MissingComponents`.
    pub fn query_one_weak<'a, Q, A: 'a>(&self, entity: WeakEntity) -> Result<Q, WeakError>
    where
        Q: Query,
    {
        todo!()
    }

    pub fn remove<T>(&mut self, e: WeakEntity)
    where
        T: Component,
    {
    }
}

pub enum WeakError {
    NoSuchEntity,
    MissingComponents,
}
