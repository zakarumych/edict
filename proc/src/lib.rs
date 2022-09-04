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
