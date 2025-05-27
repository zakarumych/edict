#[macro_export]
macro_rules! one_of {
    (@IntoQuery $q:ty) => {
        <$q as $crate::query::IntoQuery>::Query
    };
    // (@IntoQuery &'a mut $a:ty) => {
    //     $crate::query::Write<$a>
    // };
    (@Fetch $q:ty) => {
        <<$q as $crate::query::IntoQuery>::Query as $crate::query::Query>::Fetch<'a>
    };
    // (@Fetch &'a mut $a:ty) => {
    //     $crate::query::FetchWrite<'a, $a>
    // };
    (@MUTABLE $q:ty) => {
        <<$q as $crate::query::IntoQuery>::Query as $crate::query::Query>::MUTABLE
    };
    // (@MUTABLE &'a mut $a:ty) => {
    //     true
    // };
    (@DANGLING_FETCH $v:ident, $($rest:ident,)*) => {
        OneOfFetch::$v($crate::query::Fetch::dangling())
    };

    ($vis:vis $name:ident <'a> {
        $($v:ident ( $q:ty )),+ $(,)?
    }) => {
        $vis enum $name<'a> {
            $($v($q),)+
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        const _: () = {

            $vis enum OneOfFetch<'a> {
                $($v( $crate::one_of!(@Fetch $q) ),)+
            }

            #[allow(unused_parens)]
            #[allow(non_snake_case)]
            unsafe impl Fetch<'a> for OneOfFetch<'a> {
                type Item = $name<'a>;

                #[inline]
                fn dangling() -> Self {
                    $crate::one_of!(@DANGLING_FETCH $($v,)+)
                }

                #[inline]
                unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
                    match self {
                        $(OneOfFetch::$v($v) => unsafe {
                            $crate::query::Fetch::visit_chunk($v, chunk_idx)
                        },)+
                    }
                }

                /// Checks if item with specified index must be visited or skipped.
                #[inline]
                unsafe fn visit_item(&mut self, idx: u32) -> bool {
                    match self {
                        $(OneOfFetch::$v($v) => unsafe {
                            $crate::query::Fetch::visit_item($v, idx)
                        },)+
                    }
                }

                /// Notifies this fetch that it visits a chunk.
                #[inline]
                unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
                    match self {
                        $(OneOfFetch::$v($v) => unsafe {
                            $crate::query::Fetch::touch_chunk($v, chunk_idx)
                        },)+
                    }
                }

                #[inline]
                unsafe fn get_item(&mut self, idx: u32) -> $name<'a> {
                    match self {
                        $(OneOfFetch::$v($v) => unsafe {
                            $name::$v($crate::query::Fetch::get_item($v, idx))
                        },)+
                    }
                }
            }

            // $vis struct OneOf;

            // impl $crate::query::IntoQuery for $name<'_> {
            //     type Query = OneOf;
            // }

            // impl DefaultQuery for $name<'_> {
            //     #[inline]
            //     fn default_query() -> OneOf {
            //         OneOf
            //     }
            // }

            // impl $crate::query::QueryArg for $name<'_> {
            //     #[inline]
            //     fn new() -> OneOf {
            //         OneOf
            //     }
            // }

            // #[allow(non_snake_case)]
            // #[allow(unused_parens)]
            // impl $crate::query::Query for OneOf {
            //     type Item<'a> = $name<'a>;
            //     type Fetch<'a> = OneOfFetch<'a>,

            //     const MUTABLE: bool = $($crate::one_of!(@MUTABLE $q) ||)+ false;
            //     const FILTERS_ENTITIES: bool = false;

            //     fn component_access(&self, ty: core::any::TypeId) -> Result<Option<Access>, WriteAlias> {
            //         $(
            //             let $v = $crate::one_of!(@IntoQuery &'a $q);
            //             match $crate::query::Query::component_access(&$v, ty)? {
            //                 None => {},
            //                 Some(access) => return Ok(Some(access)),
            //             };
            //         )*
            //         Ok(None)
            //     }

            //     #[inline]
            //     fn visit_archetype(&self, archetype: &Archetype) -> bool {
            //         $(let $v = $crate::one_of!(@IntoQuery &'a $q);)+
            //         $( $crate::query::Query::visit_archetype(&$v, archetype) )||+
            //     }

            //     #[inline]
            //     unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
            //         $(
            //             let $v = $crate::one_of!(@IntoQuery &'a $q);
            //             if $crate::query::Query::visit_archetype(&$v, archetype) {
            //                 $crate::query::Query::access_archetype(&$v, archetype, &mut f);
            //                 return;
            //             }
            //         )+
            //     }

            //     #[inline]
            //     unsafe fn fetch<'a>(&self, arch_idx: u32, archetype: &'a Archetype, epoch: EpochId) -> OneOfFetch<'a> {
            //         $(let $v = $crate::one_of!(@IntoQuery &'a $q);)+
            //         $(
            //             if $crate::query::Query::visit_archetype(&$v, archetype) {
            //                 return OneOfFetch::$v($crate::query::Query::fetch(&$v, arch_idx, archetype, epoch));
            //             }
            //         )+
            //         unsafe {
            //             core::hint::unreachable_unchecked()
            //         }
            //     }

            //     #[inline]
            //     fn reserved_entity_item<'a>(&self, _id: EntityId, _idx: u32) -> Option<$name<'a>> {
            //         None
            //     }
            // }
        };
    };
}
