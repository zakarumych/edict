use proc_easy::EasyAttributes;
use syn::spanned::Spanned;

use crate::{kw, merge_where_clauses, Name, OnDrop, OnReplace, OnTargetDrop, WhereClause};

proc_easy::easy_attributes! {
    @(edict)
    struct RelationAttributes {
        name: Option<Name>,
        exclusive: Option<kw::exclusive>,
        symmetric: Option<kw::symmetric>,
        owned: Option<kw::owned>,
        on_drop: Option<OnDrop>,
        on_replace: Option<OnReplace>,
        on_target_drop: Option<OnTargetDrop>,
        where_clauses: Vec<WhereClause>,
    }
}

pub fn derive(
    input: syn::DeriveInput,
    edict_path: &syn::Path,
    edict_namespace: &syn::Ident,
) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let attributes = RelationAttributes::parse_in(edict_namespace, &input.attrs, input.span())?;
    let where_clause = merge_where_clauses(where_clause, &attributes.where_clauses);

    let exclusive = attributes
        .exclusive
        .map(|_| quote::quote! { const EXCLUSIVE: bool = true; });

    let symmetric = attributes
        .symmetric
        .map(|_| quote::quote! { const SYMMETRIC: bool = true; });

    let owned = attributes
        .owned
        .map(|_| quote::quote! { const OWNED: bool = true; });

    let fn_name = attributes.name.map(|name| {
        let name = name.literal;
        Some(quote::quote! {
            #[inline]
            fn name() -> &'static str {
                #name
            }
        })
    });

    let on_drop = attributes.on_drop.map(|on_drop| {
            let on_drop = &on_drop.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline]
                fn on_drop(&mut self, entity: #edict_path::entity::EntityId, target: #edict_path::entity::EntityId, encoder: &mut #edict_path::action::ActionEncoder) {
                    (#on_drop)(self, entity, target, encoder)
                }
            }
        });

    let on_replace = attributes.on_replace.map(|on_replace|{
            let on_replace = &on_replace.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline]
                fn on_replace(&mut self, value: &Self, entity: #edict_path::entity::EntityId, target: #edict_path::entity::EntityId, new_target: #edict_path::entity::EntityId, encoder: &mut #edict_path::action::ActionEncoder) -> bool {
                    (#on_replace)(self, value, entity, target, new_target, encoder)
                }
            }
        }
    );

    let on_target_drop = attributes.on_target_drop.map(|on_target_drop| {
        let on_target_drop = &on_target_drop.function;
        quote::quote! {
            #[allow(unused_variables)]
            #[inline]
            fn on_target_drop(entity: #edict_path::entity::EntityId, target: #edict_path::entity::EntityId, encoder: &mut #edict_path::action::ActionEncoder) {
                (#on_target_drop)(entity, target, encoder)
            }
        }
    });

    let output = quote::quote! {
        impl #impl_generics #edict_path::relation::Relation for #ident #ty_generics
        #where_clause
        {
            #exclusive

            #symmetric

            #owned

            #fn_name

            #on_drop

            #on_replace

            #on_target_drop
        }
    };

    Ok(output)
}
