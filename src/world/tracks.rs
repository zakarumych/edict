use crate::epoch::Epoch;

/// Value to remember which modifications was already iterated over,
/// and see what modifications are new.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_copy_implementations)]
#[repr(transparent)]
pub struct Tracks {
    pub(crate) epoch: Epoch,
}

impl Tracks {
    /// Returns new `Tracks` instance
    /// that would consider any change to be new.
    pub const fn new() -> Self {
        Tracks { epoch: 0 }
    }
}
