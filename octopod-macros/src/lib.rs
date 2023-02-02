use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, ItemFn, LitStr, Token};

struct TestParams {
    app: LitStr,
}

impl syn::parse::Parse for TestParams {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut app = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "app" if app.is_none() => {
                    let _: Token!(=) = input.parse()?;
                    app.replace(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unexpected argument: {other}"),
                    ))
                }
            }
        }

        let app = app.ok_or_else(|| {
            syn::Error::new(
                input.span(),
                format!("No app provided, test must provide app against which to run"),
            )
        })?;

        Ok(Self { app })
    }
}

#[proc_macro_attribute]
pub fn test(attr: TokenStream, input: TokenStream) -> TokenStream {
    let fun = parse_macro_input!(input as ItemFn);
    let params = parse_macro_input!(attr as TestParams);

    let fun_name = &fun.sig.ident;
    let fun_name_str = fun_name.to_string();
    let app = &params.app;

    quote! {
        octopod::sealed::inventory::submit!(
            octopod::sealed::TestDecl {
                name: concat!(module_path!(), "::", #fun_name_str),
                f: &#fun_name,
                app: #app,
            });

        #fun
    }
    .into()
}
