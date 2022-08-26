use proc_macro2::TokenStream;

mod component;

mod kw {
    proc_easy::easy_token!(name);
    proc_easy::easy_token!(borrow);
    proc_easy::easy_token!(on_drop);
    proc_easy::easy_token!(on_replace);
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
