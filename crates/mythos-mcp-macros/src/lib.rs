//! # MythOS MCP Procedural Macros
//!
//! Provides `#[mythos_mcp_tool]` attribute macro for declaring MCP tool entrypoints.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Expr, FnArg, ItemFn, Lit, Meta, MetaNameValue, PatType};

#[derive(Default)]
struct ToolMacroArgs {
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    safety_level: Option<String>,
}

fn parse_macro_args(attr: TokenStream) -> syn::Result<ToolMacroArgs> {
    let mut args = ToolMacroArgs::default();
    if attr.is_empty() {
        return Ok(args);
    }

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            let value: syn::LitStr = meta.value()?.parse()?;
            args.name = Some(value.value());
            Ok(())
        } else if meta.path.is_ident("description") {
            let value: syn::LitStr = meta.value()?.parse()?;
            args.description = Some(value.value());
            Ok(())
        } else if meta.path.is_ident("category") {
            let value: syn::LitStr = meta.value()?.parse()?;
            args.category = Some(value.value());
            Ok(())
        } else if meta.path.is_ident("safety_level") {
            let value: syn::LitStr = meta.value()?.parse()?;
            args.safety_level = Some(value.value());
            Ok(())
        } else {
            Err(meta.error("unsupported mythos_mcp_tool attribute key"))
        }
    });

    syn::parse::Parser::parse(parser, attr)?;
    Ok(args)
}

fn extract_doc_comments(attrs: &[syn::Attribute]) -> Option<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(s), ..
                    }),
                ..
            }) = &attr.meta
            {
                let val = s.value();
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    docs.push(trimmed.to_string());
                }
            }
        }
    }
    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

/// Convert snake_case identifier to PascalCase
fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in s.chars() {
        if c == '_' {
            capitalize = true;
        } else if capitalize {
            result.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    result
}

/// Attribute macro declaring an MCP tool function.
///
/// Can be invoked as:
/// ```ignore
/// #[mythos_mcp_tool(name = "read_tag", category = "telemetry", safety_level = "read_only")]
/// pub async fn read_tag(ctx: McpContext, args: ReadTagArgs) -> Result<CallToolResult, McpError> { ... }
/// ```
#[proc_macro_attribute]
pub fn mythos_mcp_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let macro_args = match parse_macro_args(attr) {
        Ok(a) => a,
        Err(e) => return TokenStream::from(e.to_compile_error()),
    };

    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_vis = &input_fn.vis;
    let fn_ident = &input_fn.sig.ident;
    let fn_name_str = fn_ident.to_string();

    let tool_name = macro_args.name.unwrap_or_else(|| fn_name_str.clone());
    let doc_desc = extract_doc_comments(&input_fn.attrs);
    let tool_desc = macro_args.description.or(doc_desc).unwrap_or_default();

    let struct_ident = format_ident!("{}Tool", to_pascal_case(&fn_name_str));
    let register_hook_ident = format_ident!("register_{}", &fn_name_str);

    let inputs = &input_fn.sig.inputs;
    let num_inputs = inputs.len();

    // Determine arguments handling
    let (call_body, schema_tokens) = match num_inputs {
        0 => (
            quote! {
                #fn_ident().await
            },
            quote! {
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                })
            },
        ),
        1 => (
            quote! {
                #fn_ident(ctx).await
            },
            quote! {
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                })
            },
        ),
        _ => {
            // Second parameter is args
            let arg_param = &inputs[1];
            let arg_type = match arg_param {
                FnArg::Typed(PatType { ty, .. }) => ty,
                _ => {
                    return TokenStream::from(
                        syn::Error::new_spanned(
                            arg_param,
                            "Expected typed parameter for tool arguments",
                        )
                        .to_compile_error(),
                    );
                }
            };

            let call = quote! {
                let typed_args: #arg_type = serde_json::from_value(args)
                    .map_err(|e| mythos_mcp::McpError::InvalidArguments(format!("Parameter schema validation error: {}", e)))?;
                #fn_ident(ctx, typed_args).await
            };

            let schema = quote! {
                serde_json::to_value(schemars::schema_for!(#arg_type))
                    .unwrap_or_else(|_| serde_json::json!({ "type": "object" }))
            };

            (call, schema)
        }
    };

    let expanded = quote! {
        #input_fn

        /// Generated MCP tool wrapper struct
        #fn_vis struct #struct_ident;

        impl #struct_ident {
            /// Generate the MCP tool manifest and input parameter schema
            pub fn tool_definition() -> mythos_mcp::Tool {
                mythos_mcp::Tool::new(
                    #tool_name,
                    if #tool_desc.is_empty() { None } else { Some(#tool_desc.to_string()) },
                    #schema_tokens,
                )
            }

            /// Execute the tool with execution context and JSON arguments
            pub async fn call(
                ctx: mythos_mcp::McpContext,
                args: serde_json::Value,
            ) -> Result<mythos_mcp::CallToolResult, mythos_mcp::McpError> {
                #call_body
            }

            /// Convenience registration hook into McpServer
            pub async fn register(server: &mythos_mcp::McpServer) {
                server.register_tool(Self::tool_definition(), std::sync::Arc::new(#struct_ident)).await;
            }
        }

        #[async_trait::async_trait]
        impl mythos_mcp::McpToolHandler for #struct_ident {
            async fn execute(
                &self,
                ctx: &mythos_mcp::McpContext,
                args: serde_json::Value,
            ) -> Result<mythos_mcp::CallToolResult, mythos_mcp::McpError> {
                #struct_ident::call(ctx.clone(), args).await
            }
        }

        /// Registration helper function for tool #fn_name_str
        #fn_vis async fn #register_hook_ident(server: &mythos_mcp::McpServer) {
            #struct_ident::register(server).await;
        }
    };

    TokenStream::from(expanded)
}
