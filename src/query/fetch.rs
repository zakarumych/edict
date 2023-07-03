use crate::archetype::chunk_idx;

/// This type can be used in [`Fetch`] implementation
/// to assert that [`Fetch`] API is used correctly.
/// Should be used only in debug mode.
#[derive(Clone, Copy)]
pub struct VerifyFetch {
    /// panics on any call if true
    dangling: bool,

    /// contains index of the last `visit_chunk` call and whether it returns true.
    visit_chunk: Option<(u32, bool)>,

    /// contains `Some` after `visit_chunk` is called.
    /// cleared in `visit_chunk` call.
    touch_chunk: Option<u32>,

    /// Contains index of the `skip_ite,` call and whether it returns true.
    visit_item: Option<(u32, bool)>,
}

impl VerifyFetch {
    /// Returns new `VerifyFetch` instance flagged as dangling.
    /// Any method call will panic. Dangling fetches must not be used.
    #[inline(always)]
    pub fn dangling() -> Self {
        VerifyFetch {
            dangling: true,
            visit_chunk: None,
            touch_chunk: None,
            visit_item: None,
        }
    }

    /// Returns new `VerifyFetch` instance.
    #[inline(always)]
    pub fn new() -> Self {
        VerifyFetch {
            dangling: false,
            visit_chunk: None,
            touch_chunk: None,
            visit_item: None,
        }
    }

    /// This method must be called in [`Fetch::visit_chunk`] implementation.
    /// `visiting` must be equal to the value returned by the [`Fetch::visit_chunk`] method.
    ///
    /// # Panics
    ///
    /// If dangling.
    #[inline(always)]
    pub fn visit_chunk(&mut self, chunk_idx: u32, visiting: bool) {
        if self.dangling {
            panic!("FetchVerify: skip_chunk called for dangling fetch");
        }
        self.visit_item = None;
        self.visit_chunk = None;
        self.visit_chunk = Some((chunk_idx, visiting));
    }

    /// This method must be called in [`Fetch::touch_chunk`] implementation.
    ///
    /// # Panics
    ///
    /// If dangling.
    /// If `visit_chunk` was not called with `visiting = true` for the same chunk just before this call.
    #[inline(always)]
    pub fn touch_chunk(&mut self, chunk_idx: u32) {
        if self.dangling {
            panic!("FetchVerify: visit_chunk called for dangling fetch");
        }
        match self.visit_chunk {
            None => {
                panic!("FetchVerify: visit_chunk called without visit_chunk");
            }
            Some((visit_chunk_idx, visiting)) => {
                if chunk_idx != visit_chunk_idx {
                    panic!("FetchVerify: visit_chunk called with chunk_idx {}, but last call to `visit_chunk` was with chunk_idx {}", chunk_idx, visit_chunk_idx);
                }
                if !visiting {
                    panic!("FetchVerify: visit_chunk called with chunk_idx {}, but `visit_chunk` returned true for this chunk index", chunk_idx);
                }
                self.touch_chunk = Some(visit_chunk_idx);
            }
        }
    }

    /// This method must be called in [`Fetch:visit_item`] implementation.
    ///
    /// # Panics
    ///
    /// If dangling.
    /// if `touch_chunk` was not called for the corresponding chunk before this call.
    #[inline(always)]
    pub fn visit_item(&mut self, idx: u32, visiting: bool) {
        if self.dangling {
            panic!("FetchVerify: visit_item called for dangling fetch");
        }
        match self.touch_chunk {
            None => {
                panic!("FetchVerify: visit_item called without visit_chunk");
            }
            Some(visit_chunk_idx) => {
                if chunk_idx(idx) != visit_chunk_idx {
                    panic!("FetchVerify: visit_item called with idx {} that correspond to chunk {}, but last call to `touch_chunk` was with chunk_idx {}", idx, chunk_idx(idx), visit_chunk_idx);
                }
                self.visit_item = Some((idx, visiting));
            }
        }
    }

    /// This method must be called in [`Fetch::get_item`] implementation.
    ///
    /// # Panics
    ///
    /// If dangling.
    /// If `visit_item` was not called with `visiting = true` for the same item just before this call.
    #[inline(always)]
    pub fn get_item(&mut self, idx: u32) {
        if self.dangling {
            panic!("FetchVerify: get_item called for dangling fetch");
        }
        match self.visit_item {
            None => {
                panic!("FetchVerify: get_item called without visit_item");
            }
            Some((valid_idx, visiting)) => {
                if idx != valid_idx {
                    panic!("FetchVerify: get_item called with idx {}, but last call to `visit_item` was with idx {}", idx, valid_idx);
                }
                if !visiting {
                    panic!("FetchVerify: get_item called with idx {}, but `visit_item` returned true for this idx", idx);
                }
                self.visit_item = None;
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
    #[must_use]
    fn dangling() -> Self;

    /// Checks if chunk with specified index must be visited or skipped.
    ///
    /// # Safety
    ///
    /// Chunk index must in range `0..=chunk_count`,
    /// where `chunk_count` is the number of chunks in the archetype
    /// from which query produced this instance.
    #[inline(always)]
    #[must_use]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        drop(chunk_idx);
        true
    }

    /// Checks if item with specified index must be visited or skipped.
    ///
    /// # Safety
    ///
    /// Entity index must in range `0..=entity_count`,
    /// where `entity_count` is the number of entities in the archetype
    /// from which query produced this instance.
    #[inline(always)]
    #[must_use]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        drop(idx);
        true
    }

    /// Notifies this fetch that at least one item in the chunk will be accessed.
    /// This method is called for each chunk in the archetype that is not skipped.
    ///
    /// # Safety
    ///
    /// Chunk index must in range `0..=chunk_count`,
    /// where `chunk_count` is the number of chunks in the archetype
    /// from which query produced this instance.
    ///
    /// `visit_chunk` must have been called just before this method.
    /// If `visit_chunk` returned `false`, this method must not be called.
    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
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
    #[must_use]
    unsafe fn get_item(&mut self, idx: u32) -> Self::Item;
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

    #[inline(always)]
    fn dangling() -> Self {
        UnitFetch {
            #[cfg(debug_assertions)]
            verify: VerifyFetch::dangling(),
        }
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let _ = chunk_idx;
        #[cfg(debug_assertions)]
        self.verify.visit_chunk(chunk_idx, true);
        true
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let _ = chunk_idx;
        #[cfg(debug_assertions)]
        self.verify.touch_chunk(chunk_idx)
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let _ = idx;
        #[cfg(debug_assertions)]
        self.verify.visit_item(idx, true);
        true
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> () {
        let _ = idx;
        #[cfg(debug_assertions)]
        self.verify.get_item(idx)
    }
}
