/// Tracking token for `Modified` query.
///
/// Created from `World`.
///
/// Users should use one for each tracking query
/// and reuse in next loop iteration for the same queries.
pub struct Tracks {
    pub(crate) epoch: u64,
}
