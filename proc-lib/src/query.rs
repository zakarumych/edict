use syn::spanned::Spanned;

pub fn derive(
    input: syn::DeriveInput,
    edict_path: &syn::Path,
    edict_namespace: &syn::Ident,
) -> syn::Result<proc_macro2::TokenStream> {
    let vis = &input.vis;
    let ident: &proc_macro2::Ident = &input.ident;
    let query_ident = quote::format_ident!("{}Query", ident);

    match input.data {
        syn::Data::Union(data) => Err(syn::Error::new_spanned(
            data.union_token,
            "Deriving `Query` is not supported for unions",
        )),
        syn::Data::Struct(data) => match data.fields {
            syn::Fields::Unit => Err(syn::Error::new_spanned(
                data.semi_token,
                "Deriving `Query` is not supported for unit structs",
            )),
            syn::Fields::Named(fields) => {
                let query_fields = fields.named.iter().map(|f| {
                    let vis = &f.vis;
                    let ident = f.ident.as_ref().unwrap();
                    let ty = &f.ty;

                    let query_ty =
                        quote::quote_spanned! { ty.span() => <#ty as #edict_path::query::AsQuery>::Query };
                    quote::quote_spanned! { f.span() => #vis #ident: #query_ty }
                });
                
                let field_vis = fields.named.iter().map(|f| {
                    &f.vis
                }).collect::<Vec<_>>();

                let field_names = fields.named.iter().map(|f| {
                    f.ident.as_ref().unwrap()
                }).collect::<Vec<_>>();

                let field_types = fields.named.iter().map(|f| {
                    &f.ty
                }).collect::<Vec<_>>();

                Ok(quote::quote! {
                    struct #query_ident {
                        #(#field_vis #query_fields,)*
                    }
                    
                    impl #edict_path::query::AsQuery for #ident {
                        type Query = #query_ident;
                    }

                    impl #edict_path::query::DefaultQuery for #ident {
                        fn default_query() -> #query_ident {
                            #query_ident{#(
                                #field_names: <#field_types as #edict_path::query::DefaultQuery>::default_query(),
                            )*}
                        }
                    }

                    impl #edict_path::query::AsQuery for #query_ident {
                        type Query = Self;
                    }

                    impl #edict_path::query::IntoQuery for #query_ident {
                        fn into_query(self) -> Self {
                            self
                        }
                    }

                    impl #edict_path::query::DefaultQuery for #query_ident {
                        fn default_query() -> #query_ident {
                            #query_ident(#(
                                <#field_types as #edict_path::query::DefaultQuery>::default_query(),
                            )*)
                        }
                    }

                    impl #edict_path::query::Query for #query_ident {
                        type Item<'a> = 
                    }
                })
            }
            syn::Fields::Unnamed(fields) => {
                let query_fields = fields.unnamed.iter().map(|f| {
                    let vis = &f.vis;
                    let ty = &f.ty;

                    quote::quote_spanned! { ty.span() => #vis <#ty as #edict_path::query::AsQuery>::Query }
                });

                let field_vis = fields.unnamed.iter().map(|f| {
                    &f.vis
                }).collect::<Vec<_>>();

                let field_indices = (0..fields.unnamed.len()).collect::<Vec<_>>();

                let field_types = fields.unnamed.iter().map(|f| {
                    &f.ty
                }).collect::<Vec<_>>();

                Ok(quote::quote! {
                    #vis struct #query_ident(
                        #(#field_vis #query_fields,)*
                    )

                    impl #edict_path::query::AsQuery for #ident {
                        type Query = #query_ident;
                    }

                    impl #edict_path::query::DefaultQuery for #ident {
                        fn default_query() -> #query_ident {
                            #query_ident(#(
                                <#field_types as #edict_path::query::DefaultQuery>::default_query(),
                            )*)
                        }
                    }

                    impl #edict_path::query::AsQuery for #query_ident {
                        type Query = Self;
                    }

                    impl #edict_path::query::IntoQuery for #query_ident {
                        fn into_query(self) -> Self {
                            self
                        }
                    }

                    impl #edict_path::query::DefaultQuery for #query_ident {
                        fn default_query() -> #query_ident {
                            #query_ident(#(
                                <#field_types as #edict_path::query::DefaultQuery>::default_query(),
                            )*)
                        }
                    }
                })
            }
        },
        syn::Data::Enum(data) => {
            let query_variants = data.variants.iter().map(|v| {
                match &v.fields {
                    syn::Fields::Unit => Err(syn::Error::new_spanned(
                        &v.ident,
                        "Deriving `Query` is not supported for unit structs",
                    )),
                    syn::Fields::Named(fields) => {
                        let query_fields = fields.named.iter().map(|f| {
                            let vis = &f.vis;
                            let ident = f.ident.as_ref().unwrap();
                            let ty = &f.ty;
        
                            let query_ty =
                                quote::quote_spanned! { ty.span() => <#ty as #edict_path::query::AsQuery>::Query };
                            quote::quote_spanned! { f.span() => #vis #ident: #query_ty }
                        });
        
                        Ok(quote::quote! {
                            struct #query_ident {
                                #(#query_fields,)*
                            }
                        })
                    }
                    syn::Fields::Unnamed(fields) => {
                        let query_fields = fields.unnamed.iter().map(|f| {
                            let vis = &f.vis;
                            let ty = &f.ty;
        
                            quote::quote_spanned! { ty.span() => #vis <#ty as #edict_path::query::AsQuery>::Query }
                        });
        
                        Ok(quote::quote! {
                            struct #query_ident(
                                #(#query_fields,)*
                            )
                        })
                    }
                }
            }).collect::<Result<Vec<_>, _>>()?;

            Ok(quote::quote! {
                #vis enum #query_ident {
                    #(#query_variants,)*
                }
            })
        }
    }
}
