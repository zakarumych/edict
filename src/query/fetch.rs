use crate::archetype::chunk_idx;

/// This type can be used in [`Fetch`] implementation
/// to assert that [`Fetch`] API is used correctly.
/// Should be used only in debug mode.
#[derive(Clone, Copy)]
pub struct VerifyFetch {
    /// panics on any call if true
    dangling: bool,

    /// contains index of the last `skip_chunk` call and whether it returns true.
    skip_chunk: Option<(usize, bool)>,

    /// contains `true` after `visit_chunk` is called.
    /// cleared in `skip_chunk` call.
    visit_chunk: Option<usize>,

    /// Contains index of the `skip_ite,` call and whether it returns true.
    skip_item: Option<(usize, bool)>,
}

impl VerifyFetch {
    /// Returns new `VerifyFetch` instance flagged as dangling.
    /// Any method call will panic. Dangling fetches must not be used.
    #[inline]
    pub fn dangling() -> Self {
        VerifyFetch {
            dangling: true,
            skip_chunk: None,
            visit_chunk: None,
            skip_item: None,
        }
    }

    /// Returns new `VerifyFetch` instance.
    #[inline]
    pub fn new() -> Self {
        VerifyFetch {
            dangling: false,
            skip_chunk: None,
            visit_chunk: None,
            skip_item: None,
        }
    }

    /// This method must be called in [`Fetch::skip_chunk`] implementation.
    /// `skipping` must be equal to the value returned by the [`Fetch::skip_chunk`] method.
    ///
    /// # Panics
    ///
    /// If dangling.
    #[inline]
    pub fn skip_chunk(&mut self, chunk_idx: usize, skipping: bool) {
        if self.dangling {
            panic!("FetchVerify: skip_chunk called for dangling fetch");
        }
        self.skip_item = None;
        self.visit_chunk = None;
        self.skip_chunk = Some((chunk_idx, skipping));
    }

    /// This method must be called in [`Fetch::visit_chunk`] implementation.
    ///
    /// # Panics
    ///
    /// If dangling.
    /// If `skip_chunk` was not called with `skipping = true` for the same chunk just before this call.
    #[inline]
    pub fn visit_chunk(&mut self, chunk_idx: usize) {
        if self.dangling {
            panic!("FetchVerify: visit_chunk called for dangling fetch");
        }
        match self.skip_chunk {
            None => {
                panic!("FetchVerify: visit_chunk called without skip_chunk");
            }
            Some((skip_chunk_idx, skipping)) => {
                if chunk_idx != skip_chunk_idx {
                    panic!("FetchVerify: visit_chunk called with chunk_idx {}, but last call to `skip_chunk` was with chunk_idx {}", chunk_idx, skip_chunk_idx);
                }
                if skipping {
                    panic!("FetchVerify: visit_chunk called with chunk_idx {}, but `skip_chunk` returned true for this chunk index", chunk_idx);
                }
                self.visit_chunk = Some(skip_chunk_idx);
            }
        }
    }

    /// This method must be called in [`Fetch::skip_item`] implementation.
    ///
    /// # Panics
    ///
    /// If dangling.
    /// if `visit_chunk` was not called for the corresponding chunk before this call.
    #[inline]
    pub fn skip_item(&mut self, idx: usize, skipping: bool) {
        if self.dangling {
            panic!("FetchVerify: skip_item called for dangling fetch");
        }
        match self.skip_chunk {
            None => {
                panic!("FetchVerify: skip_item called without skip_chunk");
            }
            Some((skip_chunk_idx, chunk_skipping)) => {
                if chunk_idx(idx) != skip_chunk_idx {
                    panic!("FetchVerify: skip_item called with idx {} that correspond to chunk {}, but last call to `visit_chunk` was with chunk_idx {}", idx, chunk_idx(idx), skip_chunk_idx);
                }
                if chunk_skipping {
                    panic!("FetchVerify: skip_item called with idx {} that correspond to chunk {}, but last call to `skip_chunk` for this chunk id returned true", idx, chunk_idx(idx));
                }
                self.skip_item = Some((idx, skipping));
            }
        }
    }

    /// This method must be called in [`Fetch::get_item`] implementation.
    ///
    /// # Panics
    ///
    /// If dangling.
    /// If `skip_item` was not called with `skipping = true` for the same item just before this call.
    #[inline]
    pub fn get_item(&mut self, idx: usize) {
        if self.dangling {
            panic!("FetchVerify: get_item called for dangling fetch");
        }
        match self.skip_item {
            None => {
                panic!("FetchVerify: get_item called without skip_item");
            }
            Some((valid_idx, skipping)) => {
                if idx != valid_idx {
                    panic!("FetchVerify: get_item called with idx {}, but last call to `skip_item` was with idx {}", idx, valid_idx);
                }
                if skipping {
                    panic!("FetchVerify: get_item called with idx {}, but `skip_item` returned true for this idx", idx);
                }
                self.skip_item = None;
            }
        }
    }
}

/// Trait implemented for `Query::Fetch` associated types.
///
/// # Safety
///
/// Implementation of unsafe methods must follow safety rules.
///
/// The order of method calls must be one of the following in call cases
///
/// 1:
/// ```ignore
/// Fetch::dangling();
/// ```
///
/// 2:
/// ```ignore
/// let fetch = Query::fetch(...)?;
///
/// for chunk_idx in 0..archetype.chunks_count() {
///   if fetch.skip_chunk(chunk_idx) && another_chunk_condition {
///     fetch.visit_chunk(chunk_idx);
///     for idx in 0..CHUNK_LEN_USIZE {
///       if fetch.skip_item(idx) && another_item_condition {
///         fetch.get_item(CHUNK_LEN_USIZE * chunk_idx + idx);
///       }
///     }
///   }
/// }
/// ```
pub unsafe trait Fetch<'a> {
    /// Item type this fetch type yields.
    type Item: 'a;

    /// Returns dummy `Fetch` value that must never be used.
    ///
    /// # Safety
    ///
    /// Implementation may return any initialized value.
    /// Even if calling any other method with it triggers an UB.
    ///
    /// Implementations are encouraged to do checks in debug mode
    /// if possible with minor performance penalty.
    fn dangling() -> Self;

    /// Checks if chunk with specified index must be skipped.
    ///
    /// # Safety
    ///
    /// Chunk index must in range `0..=chunk_count`,
    /// where `chunk_count` is the number of chunks in the archetype
    /// from which query produced this instance.
    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        drop(chunk_idx);
        false
    }

    /// Checks if item with specified index must be skipped.
    ///
    /// # Safety
    ///
    /// Entity index must in range `0..=entity_count`,
    /// where `entity_count` is the number of entities in the archetype
    /// from which query produced this instance.
    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        drop(idx);
        false
    }

    /// Notifies this fetch that it visits a new chunk.
    /// This method is called for each chunk in the archetype that is not skipped.
    ///
    /// # Safety
    ///
    /// Chunk index must in range `0..=chunk_count`,
    /// where `chunk_count` is the number of chunks in the archetype
    /// from which query produced this instance.
    ///
    /// `skip_chunk` must have been called just before this method.
    /// If `skip_chunk` returned `false`, this method must not be called.
    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        drop(chunk_idx);
    }

    /// Returns fetched item at specified index.
    ///
    /// # Safety
    ///
    /// Entity index must in range `0..=entity_count`,
    /// where `entity_count` is the number of entities in the archetype
    /// from which query produced this instance.
    ///
    /// `skip_item` must have been called just before this method.
    /// If `skip_item` returned `false`, this method must not be called.
    ///
    /// `visit_chunk` must have been called before this method
    /// with chunk index that corresponds to the entity index.
    unsafe fn get_item(&mut self, idx: usize) -> Self::Item;
}

/// Fetch type for `Query` implementations
/// where nothing needs to be fetched.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct UnitFetch {
    #[cfg(debug_assertions)]
    verify: VerifyFetch,
}

impl UnitFetch {
    /// Returns new [`UnitFetch`] instance.
    pub fn new() -> Self {
        UnitFetch {
            #[cfg(debug_assertions)]
            verify: VerifyFetch::new(),
        }
    }
}

unsafe impl<'a> Fetch<'a> for UnitFetch {
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        UnitFetch {
            #[cfg(debug_assertions)]
            verify: VerifyFetch::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let _ = chunk_idx;
        #[cfg(debug_assertions)]
        self.verify.skip_chunk(chunk_idx, false);
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let _ = chunk_idx;
        #[cfg(debug_assertions)]
        self.verify.visit_chunk(chunk_idx)
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let _ = idx;
        #[cfg(debug_assertions)]
        self.verify.skip_item(idx, false);
        false
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> () {
        let _ = idx;
        #[cfg(debug_assertions)]
        self.verify.get_item(idx)
    }
}
