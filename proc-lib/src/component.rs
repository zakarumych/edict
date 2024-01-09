use proc_easy::EasyAttributes;
use syn::spanned::Spanned;

use crate::{merge_where_clauses, Borrow, Name, OnDrop, OnReplace, WhereClause};

proc_easy::easy_attributes! {
    @(edict)
    struct ComponentAttributes {
        name: Option<Name>,
        borrow: Option<Borrow>,
        on_drop: Option<OnDrop>,
        on_replace: Option<OnReplace>,
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
    let attributes = ComponentAttributes::parse_in(edict_namespace, &input.attrs, input.span())?;
    let where_clause = merge_where_clauses(where_clause, &attributes.where_clauses);

    let fn_name = attributes.name.map(|name| {
        let name = name.literal;
        Some(quote::quote! {
            #[inline(always)]
            fn name() -> &'static str {
                #name
            }
        })
    });

    let on_drop = attributes.on_drop.map(|on_drop| {
            let on_drop = &on_drop.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline(always)]
                fn on_drop(&mut self, entity: #edict_path::entity::EntityId, encoder: #edict_path::action::LocalActionEncoder<'_>) {
                    (#on_drop)(self, entity, encoder)
                }
            }
        });

    let on_replace = attributes.on_replace.map(|on_replace|{
            let on_replace = &on_replace.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline(always)]
                fn on_replace(&mut self, value: &Self, entity: #edict_path::entity::EntityId, encoder: #edict_path::action::LocalActionEncoder<'_>) -> bool {
                    (#on_replace)(self, value, entity, encoder)
                }
            }
        }
    );

    let insert_borrows = match attributes.borrow {
        None => None,
        Some(borrow) => {
            let mut insert_borrows = quote::quote!();

            for target in borrow.targets.iter() {
                match target {
                    syn::Type::TraitObject(trait_object) => {
                        if trait_object.bounds.len() != 1 {
                            return Err(syn::Error::new(
                                target.span(),
                                "Only dyn traits without markers and lifetimes are supported",
                            ));
                        }

                        let bound =
                            match &trait_object.bounds[0] {
                                syn::TypeParamBound::Trait(bound) => bound,
                                _ => return Err(syn::Error::new(
                                    target.span(),
                                    "Only dyn traits without markers and lifetimes are supported",
                                )),
                            };

                        insert_borrows.extend(quote::quote! {
                            #edict_path::trait_borrow!(#ident as #bound => output);
                        });
                    }
                    _ => {
                        insert_borrows.extend(quote::quote! {
                            #edict_path::type_borrow!(#ident as #target => output);
                        });
                    }
                };
            }
            Some(insert_borrows)
        }
    };

    let output = quote::quote! {
        impl #impl_generics #edict_path::component::Component for #ident #ty_generics
        #where_clause
        {
            #fn_name

            #on_drop

            #on_replace

            fn borrows() -> #edict_path::private::Vec<#edict_path::component::ComponentBorrow> {
                let mut output = Vec::new();
                output.push(#edict_path::component::ComponentBorrow::auto::<Self>());
                #edict_path::trait_borrow!(Self as #edict_path::component::Value => output);
                #edict_path::trait_borrow!(Self as #edict_path::private::Any => output);
                #insert_borrows
                output
            }
        }
    };

    Ok(output)
}
