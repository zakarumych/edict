use std::str::FromStr;

use proc_macro::TokenStream;

#[proc_macro_derive(Component, attributes(edict))]
pub fn derive_component(item: TokenStream) -> TokenStream {
    let path: syn::Path = syn::parse_quote!(edict);
    edict_proc_lib::derive_component(item.into(), &path, path.get_ident().unwrap()).into()
}

#[proc_macro_derive(Relation, attributes(edict))]
pub fn derive_relation(item: TokenStream) -> TokenStream {
    let path: syn::Path = syn::parse_quote!(edict);
    edict_proc_lib::derive_relation(item.into(), &path, path.get_ident().unwrap()).into()
}

// #[proc_macro_derive(Query, attributes(edict))]
// pub fn derive_query(item: TokenStream) -> TokenStream {
//     let path: syn::Path = syn::parse_quote!(edict);
//     edict_proc_lib::derive_query(item.into(), &path, path.get_ident().unwrap()).into()
// }

/// This attribute adds checks for system functions.
/// Only applicable to function items.
///
/// Generates compilation error if function is has type parameters,
/// is async, unsafe, has return type or is a method.
///
/// Checks that all function arguments are valid system arguments.
///
/// And finally checks that function is a system.
#[proc_macro_attribute]
pub fn system(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return TokenStream::from_str(
            "::core::compile_error!(\"`#[system]` attribute doesn't take any arguments\");",
        )
        .unwrap();
    }

    let item = syn::parse_macro_input!(item as syn::ItemFn);

    let path: syn::Path = syn::parse_quote!(edict);
    match edict_proc_lib::system(item, &path) {
        Ok(output) => output.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
