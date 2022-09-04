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
                fn on_drop(&mut self, entity: #edict_path::entity::EntityId, encoder: &mut #edict_path::action::ActionEncoder) {
                    (#on_drop)(self, entity, encoder)
                }
            }
        });

    let on_replace = attributes.on_replace.map(|on_replace|{
            let on_replace = &on_replace.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline]
                fn on_replace(&mut self, value: &Self, entity: #edict_path::entity::EntityId, encoder: &mut #edict_path::action::ActionEncoder) -> bool {
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
                            {
                                #![allow(dead_code)]

                                struct DispatchBorrowSendSync<T>(DispatchBorrowSend<T>);
                                struct DispatchBorrowSend<T>(DispatchBorrowSync<T>);
                                struct DispatchBorrowSync<T>(DispatchBorrow<T>);
                                struct DispatchBorrow<T>(core::marker::PhantomData<T>);

                                impl<T> core::ops::Deref for DispatchBorrowSendSync<T> {
                                    type Target = DispatchBorrowSend<T>;

                                    fn deref(&self) -> &DispatchBorrowSend<T> {
                                        &self.0
                                    }
                                }

                                impl<T> core::ops::Deref for DispatchBorrowSend<T> {
                                    type Target = DispatchBorrowSync<T>;

                                    fn deref(&self) -> &DispatchBorrowSync<T> {
                                        &self.0
                                    }
                                }

                                impl<T> core::ops::Deref for DispatchBorrowSync<T> {
                                    type Target = DispatchBorrow<T>;

                                    fn deref(&self) -> &DispatchBorrow<T> {
                                        &self.0
                                    }
                                }

                                impl<T: #bound + Send + Sync + 'static> DispatchBorrowSendSync<T> {
                                    fn insert(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                        self.0.insert_one(extend);
                                        self.0 .0.insert_one(extend);
                                        self.0 .0 .0.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict_path::component::ComponentBorrow::make(
                                            |ptr: core::ptr::NonNull<u8>,
                                             core::marker::PhantomData|
                                             -> &(dyn #bound + Send + Sync) {
                                                unsafe { ptr.cast::<T>().as_ref() }
                                            },
                                            core::option::Option::Some(
                                                |ptr: core::ptr::NonNull<u8>,
                                                 core::marker::PhantomData|
                                                 -> &mut (dyn #bound + Send + Sync) {
                                                    unsafe { ptr.cast::<T>().as_mut() }
                                                },
                                            ),
                                        )));
                                    }
                                }

                                impl<T: #bound + Send + 'static> DispatchBorrowSend<T> {
                                    fn insert(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                        self.0 .0.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict_path::component::ComponentBorrow::make(
                                            |ptr: core::ptr::NonNull<u8>,
                                             core::marker::PhantomData|
                                             -> &(dyn #bound + Send) {
                                                unsafe { ptr.cast::<T>().as_ref() }
                                            },
                                            core::option::Option::Some(
                                                |ptr: core::ptr::NonNull<u8>,
                                                 core::marker::PhantomData|
                                                 -> &mut (dyn #bound + Send) {
                                                    unsafe { ptr.cast::<T>().as_mut() }
                                                },
                                            ),
                                        )));
                                    }
                                }

                                impl<T: #bound + Sync + 'static> DispatchBorrowSync<T> {
                                    fn insert(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                        self.0.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict_path::component::ComponentBorrow::make(
                                            |ptr: core::ptr::NonNull<u8>,
                                             core::marker::PhantomData|
                                             -> &(dyn #bound + Sync) {
                                                unsafe { ptr.cast::<T>().as_ref() }
                                            },
                                            core::option::Option::Some(
                                                |ptr: core::ptr::NonNull<u8>,
                                                 core::marker::PhantomData|
                                                 -> &mut (dyn #bound + Sync) {
                                                    unsafe { ptr.cast::<T>().as_mut() }
                                                },
                                            ),
                                        )));
                                    }
                                }

                                impl<T: #bound + 'static> DispatchBorrow<T> {
                                    fn insert(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict_path::private::Vec<#edict_path::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict_path::component::ComponentBorrow::make(
                                            |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &dyn #bound {
                                                unsafe { ptr.cast::<T>().as_ref() }
                                            },
                                            core::option::Option::Some(
                                                |ptr: core::ptr::NonNull<u8>,
                                                 core::marker::PhantomData|
                                                 -> &mut dyn #bound {
                                                    unsafe { ptr.cast::<T>().as_mut() }
                                                },
                                            ),
                                        )));
                                    }
                                }

                                let dispatch = DispatchBorrowSendSync(DispatchBorrowSend(DispatchBorrowSync(
                                    DispatchBorrow(core::marker::PhantomData::<#ident>),
                                )));
                                dispatch.insert(&mut output);
                            }
                        });
                    }
                    _ => {
                        insert_borrows.extend(quote::quote! {
                            let dispatch = #edict_path::component::private::DispatchBorrowMut(#edict_path::component::private::DispatchBorrow(core::marker::PhantomData::<(#ident, #target)>));
                            dispatch.insert(&mut output);
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
                #edict_path::borrow_dyn_any!(Self => output);
                #insert_borrows
                output
            }
        }
    };

    Ok(output)
}
