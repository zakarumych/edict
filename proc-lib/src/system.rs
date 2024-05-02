use proc_macro2::TokenStream;
use syn::spanned::Spanned;

pub fn system(mut item: syn::ItemFn, edict_path: &syn::Path) -> syn::Result<TokenStream> {
    if item.sig.generics.type_params().count() != 0 {
        return Err(syn::Error::new_spanned(
            item.sig.generics,
            "functions with type parameters may not be used with `#[system]` attribute",
        ));
    }

    if item.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            item.sig.fn_token,
            "async functions may not be used with `#[system]` attribute",
        ));
    }

    if item.sig.unsafety.is_some() {
        return Err(syn::Error::new_spanned(
            item.sig.unsafety,
            "unsafe functions may not be used with `#[system]` attribute",
        ));
    }

    if item.sig.output != syn::ReturnType::Default {
        return Err(syn::Error::new_spanned(
            item.sig.output,
            "functions with return types may not be used with `#[system]` attribute",
        ));
    }

    let mut checks = Vec::new();

    // let where_clause = item.sig.generics.make_where_clause();

    for arg in item.sig.inputs.iter() {
        match arg {
            syn::FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    arg,
                    "methods may not be used with `#[system]` attribute",
                ));
            }

            syn::FnArg::Typed(arg) => {
                let ty = &*arg.ty;
                checks.push(syn::parse_quote_spanned!(ty.span() => {
                    #edict_path::private::is_fn_arg::<#ty>();
                    // #edict_path::private::is_fn_system(|_: #ty| {});
                }));

                // where_clause
                //     .predicates
                //     .push(syn::parse_quote_spanned!(ty.span() => #ty: #edict_path::private::FnArg));
            }
        }
    }

    let ident = &item.sig.ident;

    checks.push(
        syn::parse_quote_spanned!(ident.span() => #edict_path::private::is_fn_system(#ident);),
    );

    checks.append(&mut item.block.stmts);
    item.block.stmts = checks;

    Ok(quote::quote!(#item))
}
