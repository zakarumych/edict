use alloc::{boxed::Box, vec};
use core::{any::TypeId, fmt::Debug};

use crate::hash::no_op_hash;

#[derive(Debug)]
pub struct TypeIdSet {
    count: usize,
    modulo: usize,
    entries: Box<[TypeId]>,
}

impl TypeIdSet {
    /// Returns TypeIdSet with given type ids.
    pub fn new(ids: impl Iterator<Item = TypeId> + Clone) -> Self {
        let no_type_id = no_type_id();

        let mut entries = vec![no_type_id; ids.clone().count()];

        'outer: loop {
            for id in ids.clone() {
                assert_ne!(id, no_type_id);

                let idx = type_index(&id, entries.len());

                if entries[idx] == no_type_id {
                    entries[idx] = id;
                } else if entries[idx] == id {
                    panic!("Duplicate type id");
                } else {
                    let len = entries.len() + 1;
                    let add = len - entries.len();
                    entries.clear();
                    entries.reserve_exact(add);
                    entries.resize(len, no_type_id);
                    continue 'outer;
                }
            }

            let modulo = entries.len().max(1);

            while entries.last() == Some(&no_type_id) {
                entries.pop();
            }

            return TypeIdSet {
                count: ids.count(),
                modulo,
                entries: entries.into_boxed_slice(),
            };
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn upper_bound(&self) -> usize {
        self.entries.len()
    }

    /// Returns `Some(idx)` where `idx` is index of the type id in the set.
    /// Returns `None` if id is not in the set.
    #[inline]
    pub fn get(&self, id: TypeId) -> Option<usize> {
        let idx = type_index(&id, self.modulo);
        if self.entries.get(idx) == Some(&id) {
            Some(idx)
        } else {
            None
        }
    }

    /// Returns `Some(idx)` where `idx` is index of the type id in the set.
    /// Returns `None` if id is not in the set.
    #[inline]
    pub fn get_unchecked(&self, id: TypeId) -> usize {
        type_index(&id, self.modulo)
    }

    /// Returns `true` if id is in the set.
    /// Returns `false` if id is not in the set.
    #[inline]
    pub fn contains_id(&self, id: TypeId) -> bool {
        let idx = type_index(&id, self.modulo);
        if self.entries.get(idx) == Some(&id) {
            true
        } else {
            false
        }
    }

    // pub fn ids(&self) -> TypeIdSetIter<'_> {
    //     TypeIdSetIter {
    //         count: self.count,
    //         inner: self.entries.iter(),
    //     }
    // }

    #[inline]
    pub fn indexed(&self) -> TypeIdSetIndexedIter<'_> {
        TypeIdSetIndexedIter {
            count: self.count,
            inner: self.entries.iter().enumerate(),
        }
    }
}

pub struct TypeIdSetIter<'a> {
    count: usize,
    inner: core::slice::Iter<'a, TypeId>,
}

impl Iterator for TypeIdSetIter<'_> {
    type Item = TypeId;

    fn next(&mut self) -> Option<TypeId> {
        if self.count == 0 {
            None
        } else {
            let no_type_id = no_type_id();
            loop {
                let id = *self.inner.next().unwrap();
                if id != no_type_id {
                    self.count -= 1;
                    return Some(id);
                }
            }
        }
    }

    fn count(self) -> usize {
        self.count
    }

    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, TypeId) -> B,
    {
        let no_type_id = no_type_id();
        self.inner.fold(
            init,
            |init, &id| {
                if id != no_type_id {
                    f(init, id)
                } else {
                    init
                }
            },
        )
    }

    fn for_each<F>(self, mut f: F)
    where
        Self: Sized,
        F: FnMut(TypeId),
    {
        let no_type_id = no_type_id();
        self.inner.for_each(|&id| {
            if id != no_type_id {
                f(id);
            }
        })
    }
}

impl ExactSizeIterator for TypeIdSetIter<'_> {
    fn len(&self) -> usize {
        self.count
    }
}

impl DoubleEndedIterator for TypeIdSetIter<'_> {
    fn next_back(&mut self) -> Option<TypeId> {
        if self.count == 0 {
            None
        } else {
            let no_type_id = no_type_id();
            loop {
                let id = *self.inner.next_back().unwrap();
                if id != no_type_id {
                    self.count -= 1;
                    return Some(id);
                }
            }
        }
    }

    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, TypeId) -> B,
    {
        let no_type_id = no_type_id();
        self.inner.rfold(
            init,
            |init, &id| {
                if id != no_type_id {
                    f(init, id)
                } else {
                    init
                }
            },
        )
    }
}

pub struct TypeIdSetIndexedIter<'a> {
    count: usize,
    inner: core::iter::Enumerate<core::slice::Iter<'a, TypeId>>,
}

impl Iterator for TypeIdSetIndexedIter<'_> {
    type Item = (usize, TypeId);

    fn next(&mut self) -> Option<(usize, TypeId)> {
        if self.count == 0 {
            None
        } else {
            let no_type_id = no_type_id();
            loop {
                let (idx, &id) = self.inner.next().unwrap();
                if id != no_type_id {
                    self.count -= 1;
                    return Some((idx, id));
                }
            }
        }
    }

    fn count(self) -> usize {
        self.count
    }

    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, (usize, TypeId)) -> B,
    {
        let no_type_id = no_type_id();
        self.inner.fold(init, |init, (idx, &id)| {
            if id != no_type_id {
                f(init, (idx, id))
            } else {
                init
            }
        })
    }

    fn for_each<F>(self, mut f: F)
    where
        Self: Sized,
        F: FnMut((usize, TypeId)),
    {
        let no_type_id = no_type_id();
        self.inner.for_each(|(idx, &id)| {
            if id != no_type_id {
                f((idx, id));
            }
        })
    }
}

impl ExactSizeIterator for TypeIdSetIndexedIter<'_> {
    fn len(&self) -> usize {
        self.count
    }
}

impl DoubleEndedIterator for TypeIdSetIndexedIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            let no_type_id = no_type_id();
            loop {
                let (idx, &id) = self.inner.next_back().unwrap();
                if id != no_type_id {
                    self.count -= 1;
                    return Some((idx, id));
                }
            }
        }
    }

    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, (usize, TypeId)) -> B,
    {
        let no_type_id = no_type_id();
        self.inner.rfold(init, |init, (idx, &id)| {
            if id != no_type_id {
                f(init, (idx, id))
            } else {
                init
            }
        })
    }
}

/// This function returns opaque TypeId which is treated as none
/// by components code.
fn no_type_id() -> TypeId {
    pub struct NoThisIsPatrik;
    TypeId::of::<NoThisIsPatrik>()
}

fn type_index(id: &TypeId, len: usize) -> usize {
    no_op_hash(id).swap_bytes() as usize % len
}
