//! # MythOS MCP Procedural Macros
//!
//! Provides `#[mythos_mcp_tool]` attribute macro for declaring MCP tool entrypoints.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Attribute macro declaring an MCP tool function.
#[proc_macro_attribute]
pub fn mythos_mcp_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let _fn_vis = &input.vis;
    let _fn_block = &input.block;

    let expanded = quote! {
        #input

        // Registration hook stub
        #[doc(hidden)]
        pub fn __mythos_mcp_register_tool() -> &'static str {
            stringify!(#fn_name)
        }
    };

    TokenStream::from(expanded)
}
