use proc_macro2::TokenStream;

mod component;
mod relation;

mod kw {
    proc_easy::easy_token!(name);
    proc_easy::easy_token!(borrow);
    proc_easy::easy_token!(on_drop);
    proc_easy::easy_token!(on_target_drop);
    proc_easy::easy_token!(on_replace);
    proc_easy::easy_token!(exclusive);
    proc_easy::easy_token!(symmetric);
    proc_easy::easy_token!(owned);
}

proc_easy::easy_argument_value! {
    struct Name {
        kw: kw::name,
        literal: syn::LitStr,
    }
}

proc_easy::easy_argument! {
    struct Borrow {
        kw: kw::borrow,
        targets: proc_easy::EasyParenthesized<proc_easy::EasyTerminated<syn::Type>>,
    }
}

proc_easy::easy_argument! {
    struct OnDrop {
        kw: kw::on_drop,
        function: syn::Expr,
    }
}

proc_easy::easy_argument! {
    struct OnReplace {
        kw: kw::on_replace,
        function: syn::Expr,
    }
}

proc_easy::easy_argument! {
    struct OnTargetDrop {
        kw: kw::on_target_drop,
        function: syn::Expr,
    }
}

proc_easy::easy_argument! {
    struct Exclusive {
        kw: kw::exclusive,
    }
}

proc_easy::easy_argument! {
    struct Owned {
        kw: kw::owned,
    }
}

proc_easy::easy_argument! {
    struct WhereClause {
        kw: syn::Token![where],
        predicates: proc_easy::EasyTerminated<syn::WherePredicate, syn::Token![,]>,
    }
}

fn merge_where_clauses(
    where_clause: Option<&syn::WhereClause>,
    additional: &[WhereClause],
) -> Option<syn::WhereClause> {
    match where_clause {
        None if additional.is_empty() => None,
        None => {
            let mut predicates = syn::punctuated::Punctuated::new();

            for where_clause in additional {
                for predicate in where_clause.predicates.iter() {
                    predicates.push(predicate.clone());
                }
            }

            Some(syn::WhereClause {
                where_token: additional[0].kw,
                predicates,
            })
        }
        Some(where_clause) => {
            let mut predicates = where_clause.predicates.clone();

            for where_clause in additional {
                for predicate in where_clause.predicates.iter() {
                    predicates.push(predicate.clone());
                }
            }

            Some(syn::WhereClause {
                where_token: where_clause.where_token,
                predicates,
            })
        }
    }
}

pub fn derive_component(
    item: TokenStream,
    edict_path: &syn::Path,
    edict_namespace: &syn::Ident,
) -> TokenStream {
    match syn::parse2(item).and_then(|input| component::derive(input, edict_path, edict_namespace))
    {
        Ok(output) => output.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

pub fn derive_relation(
    item: TokenStream,
    edict_path: &syn::Path,
    edict_namespace: &syn::Ident,
) -> TokenStream {
    match syn::parse2(item).and_then(|input| relation::derive(input, edict_path, edict_namespace)) {
        Ok(output) => output.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
