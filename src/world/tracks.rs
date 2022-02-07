/// Value to remember which modifications was already iterated over,
/// and see what modifications are new.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_copy_implementations)]
#[repr(transparent)]
pub struct Tracks {
    pub(crate) epoch: u64,
}
