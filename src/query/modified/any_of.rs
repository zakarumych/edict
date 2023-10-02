use crate::query::{
    any_of::AnyOf, boolean::BooleanQuery, boolean::OrOp, read::Read, write::Write, IntoQuery,
};

use super::Modified;

macro_rules! any_of {
    () => { /* Don't implement for empty tuple */ };
    ($($a:ident)+) => {
        impl<$($a),+> IntoQuery for Modified<AnyOf<($(&$a,)+)>>
        where
            $($a: Sync + 'static,)+
        {
            type Query = BooleanQuery<($(Modified<Read<$a>>,)+), OrOp>;

            fn into_query(self) -> Self::Query {
                BooleanQuery::from_tuple(($(Modified::<Read<$a>>::new(self.after_epoch),)*))
            }
        }

        impl<$($a),+> IntoQuery for Modified<AnyOf<($(Read<$a>,)+)>>
        where
            $($a: Sync + 'static,)+
        {
            type Query = BooleanQuery<($(Modified<Read<$a>>,)+), OrOp>;

            fn into_query(self) -> Self::Query {
                BooleanQuery::from_tuple(($(Modified::<Read<$a>>::new(self.after_epoch),)*))
            }
        }

        impl<$($a),+> IntoQuery for Modified<AnyOf<($(&mut $a,)+)>>
        where
            $($a: Send + 'static,)+
        {
            type Query = BooleanQuery<($(Modified<Write<$a>>,)+), OrOp>;

            fn into_query(self) -> Self::Query {
                BooleanQuery::from_tuple(($(Modified::<Write<$a>>::new(self.after_epoch),)*))
            }
        }

        impl<$($a),+> IntoQuery for Modified<AnyOf<($(Write<$a>,)+)>>
        where
            $($a: Send + 'static,)+
        {
            type Query = BooleanQuery<($(Modified<Write<$a>>,)+), OrOp>;

            fn into_query(self) -> Self::Query {
                BooleanQuery::from_tuple(($(Modified::<Write<$a>>::new(self.after_epoch),)*))
            }
        }
    };
}

for_tuple!(any_of);
