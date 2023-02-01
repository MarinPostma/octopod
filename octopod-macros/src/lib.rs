use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn test(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let fun = parse_macro_input!(input as ItemFn);
    let fun_name = &fun.sig.ident;
    let fun_name_str = fun_name.to_string();
    quote! {
        octopod::sealed::inventory::submit!(
            octopod::sealed::TestDecl {
                name: #fun_name_str,
                f: &#fun_name,
                target_apps: &[],
            });

        #fun
    }
    .into()
}
