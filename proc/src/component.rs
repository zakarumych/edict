use proc_easy::EasyAttributes;
use syn::spanned::Spanned;

use crate::kw;

proc_easy::easy_argument_value! {
    struct Edict {
        kw: kw::edict,
        path: syn::Path,
    }
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

proc_easy::easy_attributes! {
    @(edict)
    struct ComponentAttributes {
        edict: Option<Edict>,
        name: Option<Name>,
        borrow: Option<Borrow>,
        on_drop: Option<OnDrop>,
        on_replace: Option<OnReplace>,
    }
}

pub fn derive(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let attributes = ComponentAttributes::parse(&input.attrs, input.span())?;

    let edict = match attributes.edict {
        None => syn::parse_quote! { edict },
        Some(edict) => edict.path,
    };

    let fn_name = match attributes.name {
        None => quote::quote!(),
        Some(name) => {
            let name = name.literal;
            quote::quote! {
                #[inline]
                fn name() -> &'static str {
                    #name
                }
            }
        }
    };

    let insert_borrows = match attributes.borrow {
        None => quote::quote!(),
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
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                        self.0.insert_one(extend);
                                        self.0 .0.insert_one(extend);
                                        self.0 .0 .0.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict::component::ComponentBorrow::make(
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
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                        self.0 .0.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict::component::ComponentBorrow::make(
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
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                        self.0.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict::component::ComponentBorrow::make(
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
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        self.insert_one(extend);
                                    }

                                    fn insert_one(
                                        &self,
                                        extend: &mut #edict::private::Vec<#edict::component::ComponentBorrow>,
                                    ) {
                                        extend.extend(Some(#edict::component::ComponentBorrow::make(
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
                            let dispatch = #edict::component::private::DispatchBorrowMut(#edict::component::private::DispatchBorrow(core::marker::PhantomData::<(#ident, #target)>));
                            dispatch.insert(&mut output);
                        });
                    }
                };
            }
            insert_borrows
        }
    };

    let on_drop = match &attributes.on_drop {
        None => quote::quote!(),
        Some(on_drop) => {
            let on_drop = &on_drop.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline]
                fn on_drop(&mut self, entity: #edict::entity::EntityId, encoder: &mut #edict::action::ActionEncoder) {
                    (#on_drop)(self, entity, encoder)
                }
            }
        }
    };

    let on_replace = match &attributes.on_replace {
        None => quote::quote!(),
        Some(on_replace) => {
            let on_replace = &on_replace.function;
            quote::quote! {
                #[allow(unused_variables)]
                #[inline]
                fn on_replace(&mut self, value: &Self, entity: #edict::entity::EntityId, encoder: &mut #edict::action::ActionEncoder) -> bool {
                    (#on_replace)(self, value, entity, encoder)
                }
            }
        }
    };

    let output = quote::quote! {
        impl #impl_generics #edict::component::Component for #ident #ty_generics
        #where_clause
        {
            #fn_name

            #on_drop

            #on_replace

            fn borrows() -> #edict::private::Vec<#edict::component::ComponentBorrow> {
                let mut output = Vec::new();
                output.push(#edict::component::ComponentBorrow::auto::<Self>());
                #edict::borrow_dyn_any!(Self => output);
                #insert_borrows
                output
            }
        }
    };

    Ok(output)
}
