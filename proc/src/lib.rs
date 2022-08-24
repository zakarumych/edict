use proc_macro::TokenStream;

mod component;

#[proc_macro_derive(Component, attributes(edict))]
pub fn derive_component(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    match component::derive(input) {
        Ok(output) => output.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

mod kw {
    proc_easy::easy_token!(edict);
    proc_easy::easy_token!(name);
    proc_easy::easy_token!(borrow);
    proc_easy::easy_token!(on_drop);
    proc_easy::easy_token!(on_replace);
}
