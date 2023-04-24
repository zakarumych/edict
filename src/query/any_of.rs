use super::{
    boolean::{BooleanQuery, OrOp},
    read::{read, Read},
    IntoQuery,
};

/// A query adaptor parameterized by a tuple of queries.
/// Yields a tuple of items from each query wrapped in `Option`.
/// Yields `None` for queries that do not match the entity.
/// Skips if no queries match the entity.
pub struct AnyOf<T>(T);

macro_rules! any_of {
    () => { /* Don't implement for empty tuple */ };
    ($($a:ident)+) => {
        impl<$($a),+> IntoQuery for AnyOf<($($a,)+)>
        where
            $($a: Sync + 'static,)+
        {
            type Query = BooleanQuery<($(Read<$a>,)+), OrOp>;

            fn into_query(self) -> Self::Query {
                BooleanQuery::from_tuple(($(read::<$a>(),)*))
            }
        }
    };
}

for_tuple!(any_of);
