use proc_macro::{Ident, TokenStream, TokenTree};

#[proc_macro_derive(PinComponent)]
pub fn derive_pin_component(item: TokenStream) -> TokenStream {
    match get_type_name(item) {
        Ok(name) => format!("impl ::edict::proc_export::PinUnpinSwitch for {name} {{ type Switch = ::edict::proc_export::PinSwitch; }} impl ::edict::PinComponent for {name} {{}}")
            .parse()
            .unwrap(),
        Err(err) => err,
    }
}

#[proc_macro_derive(UnpinComponent)]
pub fn derive_unpin_component(item: TokenStream) -> TokenStream {
    match get_type_name(item) {
        Ok(name) => format!("impl ::edict::proc_export::PinUnpinSwitch for {name} {{ type Switch = ::edict::proc_export::UnpinSwitch; }} impl ::edict::UnpinComponent for {name} {{}}")
            .parse()
            .unwrap(),
        Err(err) => err,
    }
}

fn get_type_name(item: TokenStream) -> Result<Ident, TokenStream> {
    let mut skip_group = false;
    let mut expect_name = false;
    for tt in item.into_iter() {
        match tt {
            TokenTree::Punct(p) if p == '#' => {
                skip_group = true;
            }
            TokenTree::Punct(p) => {
                return Err(format!("::std::compile_error!(\"Unexpected token {}\")", p)
                    .parse()
                    .unwrap())
            }
            TokenTree::Group(_) if skip_group => {
                skip_group = false;
            }
            TokenTree::Group(g) => {
                return Err(format!("::std::compile_error!(\"Unexpected group {}\")", g)
                    .parse()
                    .unwrap())
            }
            TokenTree::Literal(l) => {
                return Err(
                    format!("::std::compile_error!(\"Unexpected literal {}\")", l)
                        .parse()
                        .unwrap(),
                )
            }
            TokenTree::Ident(i) if expect_name => return Ok(i),
            TokenTree::Ident(i) => match &*i.to_string() {
                "struct" | "enum" | "union" => {
                    expect_name = true;
                }
                _ => {
                    return Err(format!("::std::compile_error!(\"Unexpected ident {}\")", i)
                        .parse()
                        .unwrap())
                }
            },
        }
    }

    Err(
        format!("::std::compile_error!(\"Unexpected end of input\")")
            .parse()
            .unwrap(),
    )
}
