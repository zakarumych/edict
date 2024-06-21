use proc_macro2::TokenStream;
use syn::spanned::Spanned;

pub fn flow_fn(closure: syn::ExprClosure, edict_path: &syn::Path) -> syn::Result<TokenStream> {
    if closure.inputs.len() != 1 {
        return Err(syn::Error::new(
            closure.span(),
            "expected a closure with exactly one argument",
        ));
    }

    match closure.output {
        syn::ReturnType::Default => {}
        _ => {
            return Err(syn::Error::new(
                closure.output.span(),
                "expected a closure with no return type",
            ));
        }
    }

    let arg = &closure.inputs[0];
    let body = closure.body;

    Ok(quote::quote! {
        unsafe {
            #edict_path::flow::FlowClosure::new(move |token| async move {
                #[allow(unused)]
                let #arg = #edict_path::flow::FlowContext::cx(&token);
                {
                    #body
                }
            })
        }
    })
}
