use crate::query::{any_of::AnyOf, boolean::BooleanQuery, boolean::OrOp, IntoQuery};

use super::Modified;

macro_rules! any_of {
    () => { /* Don't implement for empty tuple */ };
    ($($a:ident)+) => {
        impl<$($a),+> IntoQuery for Modified<AnyOf<($(&$a,)+)>>
        where
            $($a: Sync + 'static,)+
        {
            type Query = BooleanQuery<($(Modified<&'static $a>,)+), OrOp>;

            fn into_query(self) -> Self::Query {
                BooleanQuery::from_tuple(($(Modified::<&$a>::new(self.after_epoch),)*))
            }
        }
    };
}

for_tuple!(any_of);
