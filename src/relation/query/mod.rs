//!
//! This module contains queries for relations.
//!
//! Naming rules.
//!
//! `*Relates*` - matches origins of relations.
//! `*Related*` - matches targets of relations.
//! `*RelatesTo` - contains specific relation target.
//! `*RelatedBy` - contains specific relation origin.
//!
//! # Queries
//!
//! [`Relates`] - matches relation origins and fetches slice of relation instances and targets.
//! [`RelatesExclusive`] - matches relation origins and fetches exclusive relation instance and target.
//! [`RelatesTo`] - matches relation origin with specified target and fetches relation instance.
//! [`Related`] - matches relation targets and fetches slice of origins.
//!
//! # Filters
//!
//! [`FilterRelates`] - filters relation targets.
//! [`FilterRelatesTo`] - filters relations targets with specified origin.
//! [`FilterNotRelates`] - filters entities that are not relation targets.
//! [`FilterNotRelatesTo`] - filters entities that are not relation targets with specified origin.
//!
//! [`FilterRelated`] - filters relation targets.
//! [`FilterRelatedBy`] - filters relations targets with specified origin.
//! [`FilterNotRelated`] - filters entities that are not relation targets.
//! [`FilterNotRelatedBy`] - filters entities that are not relation targets with specified origin.

mod filter_related;
mod filter_related_by;
mod filter_relates;
mod filter_relates_to;
mod related;
mod relates;
mod relates_exclusive;
mod relates_to;

pub use self::{
    filter_related::FilterRelated,
    filter_related_by::{FetchFilterRelatedBy, FilterRelatedBy},
    filter_relates::FilterRelates,
    filter_relates_to::{FilterFetchRelatesTo, FilterRelatesTo},
    related::{FetchRelated, Related},
    relates::{FetchRelatesRead, FetchRelatesWrite, Relates, RelatesReadIter, RelatesWriteIter},
    relates_exclusive::{FetchRelatesExclusiveRead, FetchRelatesExclusiveWrite, RelatesExclusive},
    relates_to::{FetchRelatesToRead, FetchRelatesToWrite, RelatesTo},
};
