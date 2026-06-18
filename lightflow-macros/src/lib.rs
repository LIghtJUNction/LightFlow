use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    FnArg, Ident, ItemFn, LitStr, Pat, PatIdent, Path, Result, ReturnType, Token, Type,
    parse_macro_input,
};

#[proc_macro_attribute]
pub fn node(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as NodeArgs);
    let function = parse_macro_input!(item as ItemFn);
    expand_node(args, function).into()
}

#[proc_macro_attribute]
pub fn trace_node(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as ItemFn);
    let name = function.sig.ident.to_string();
    expand_node(
        NodeArgs {
            name: LitStr::new(&name, function.sig.ident.span()),
            disabled: None,
        },
        function,
    )
    .into()
}

#[proc_macro_attribute]
pub fn workflow(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as WorkflowArgs);
    let function = parse_macro_input!(item as ItemFn);
    expand_workflow(args, function, false).into()
}

#[proc_macro_attribute]
pub fn subworkflow(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as WorkflowArgs);
    let function = parse_macro_input!(item as ItemFn);
    expand_workflow(args, function, true).into()
}

#[proc_macro_attribute]
pub fn retry(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn timeout(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_derive(WorkflowInput)]
pub fn workflow_input(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::DeriveInput);
    let name = input.ident;
    quote! {
        impl #name {
            pub fn workflow_input_schema() -> ::lightflow::serde_json::Value {
                ::lightflow::serde_json::json!({
                    "title": stringify!(#name),
                    "type": "object"
                })
            }

            pub fn workflow_input_template() -> ::lightflow::serde_json::Value {
                ::lightflow::serde_json::json!({})
            }
        }
    }
    .into()
}

#[proc_macro_derive(WorkflowOutput)]
pub fn workflow_output(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::DeriveInput);
    let name = input.ident;
    quote! {
        impl #name {
            pub fn workflow_output_schema() -> ::lightflow::serde_json::Value {
                ::lightflow::serde_json::json!({
                    "title": stringify!(#name),
                    "type": "object"
                })
            }
        }
    }
    .into()
}

struct NodeArgs {
    name: LitStr,
    disabled: Option<Path>,
}

impl Parse for NodeArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name = input.parse::<LitStr>()?;
        let mut disabled = None;
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            let key = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            if key == "disabled" {
                let value = input.parse::<LitStr>()?;
                disabled = Some(value.parse()?);
            } else {
                return Err(syn::Error::new(key.span(), "unsupported node option"));
            }
        }
        Ok(Self { name, disabled })
    }
}

struct WorkflowArgs {
    name: LitStr,
}

impl Parse for WorkflowArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
        })
    }
}

fn expand_node(args: NodeArgs, function: ItemFn) -> proc_macro2::TokenStream {
    if let (Some((input_name, input_ty)), Some(output_ty)) = (
        parse_workflow_input(&function),
        parse_workflow_output(&function),
    ) {
        return expand_typed_node(args, function, input_name, input_ty, output_ty);
    }
    expand_traced_node(args, function)
}

fn expand_traced_node(args: NodeArgs, mut function: ItemFn) -> proc_macro2::TokenStream {
    let name = args.name;
    let block = function.block;
    let arg_patterns = function
        .sig
        .inputs
        .iter()
        .filter_map(fn_arg_pattern)
        .collect::<Vec<_>>();
    let disabled = args.disabled.map(|fallback| {
        quote! {
            if ::lightflow::trace::node_is_disabled(__lightflow_node_name) {
                ::lightflow::trace::node_disabled(__lightflow_node_name);
                return #fallback(#(#arg_patterns),*).await;
            }
        }
    });
    function.block = Box::new(syn::parse_quote!({
        let __lightflow_node_name = #name;
        #disabled
        let __lightflow_started_at = ::lightflow::trace::node_start(__lightflow_node_name);
        let __lightflow_result = (async move #block).await;
        match __lightflow_result {
            Ok(__lightflow_output) => {
                ::lightflow::trace::node_end(__lightflow_node_name, __lightflow_started_at);
                Ok(__lightflow_output)
            }
            Err(__lightflow_error) => {
                ::lightflow::trace::node_error(
                    __lightflow_node_name,
                    __lightflow_started_at,
                    &__lightflow_error,
                );
                Err(__lightflow_error.context(format!(
                    "node `{}` failed",
                    __lightflow_node_name
                )))
            }
        }
    }));
    quote! { #function }
}

fn expand_typed_node(
    args: NodeArgs,
    mut function: ItemFn,
    input_name: PatIdent,
    input_ty: Type,
    output_ty: Type,
) -> proc_macro2::TokenStream {
    let node_name = args.name;
    let visibility = function.vis.clone();
    let original_name = function.sig.ident.clone();
    let implementation_name = format_ident!("__lightflow_node_{}_impl", original_name);
    let with_hooks_name = format_ident!("{}_with_hooks", original_name);
    function.sig.ident = implementation_name.clone();
    function.vis = syn::parse_quote!(pub);
    let fallback_call = args.disabled.map(|fallback| {
        quote! {
            ::lightflow::patch::run_node_with_fallback(
                #node_name,
                #input_name,
                |#input_name: #input_ty| async move {
                    #implementation_name(#input_name).await
                },
                |#input_name: #input_ty| async move {
                    #fallback(#input_name).await
                },
                hooks,
            ).await
        }
    });
    let run_call = fallback_call.unwrap_or_else(|| {
        quote! {
            ::lightflow::patch::run_node(
                #node_name,
                #input_name,
                |#input_name: #input_ty| async move {
                    #implementation_name(#input_name).await
                },
                hooks,
            ).await
        }
    });
    quote! {
        #function

        #visibility async fn #original_name(
            #input_name: #input_ty,
        ) -> ::lightflow::anyhow::Result<#output_ty> {
            let hooks: ::lightflow::patch::HookRegistry<#input_ty, #output_ty> =
                if ::lightflow::trace::node_is_disabled(#node_name) {
                    ::lightflow::patch::HookRegistry::new().disable(#node_name)
                } else {
                    ::lightflow::patch::HookRegistry::new()
                };
            #with_hooks_name(#input_name, &hooks).await
        }

        #visibility async fn #with_hooks_name(
            #input_name: #input_ty,
            hooks: &::lightflow::patch::HookRegistry<#input_ty, #output_ty>,
        ) -> ::lightflow::anyhow::Result<#output_ty> {
            #run_call
        }
    }
}

fn expand_workflow(
    args: WorkflowArgs,
    mut function: ItemFn,
    subworkflow: bool,
) -> proc_macro2::TokenStream {
    let workflow_name = args.name;
    let visibility = function.vis.clone();
    let original_name = function.sig.ident.clone();
    let runner_name = original_name.clone();
    let implementation_name = format_ident!("__lightflow_workflow_{}_impl", original_name);
    function.sig.ident = implementation_name.clone();
    function.vis = syn::parse_quote!(pub);
    let Some((input_name, input_ty)) = parse_workflow_input(&function) else {
        return syn::Error::new_spanned(
            function.sig.inputs,
            "workflow functions must take exactly one typed input argument",
        )
        .to_compile_error();
    };
    let Some(output_ty) = parse_workflow_output(&function) else {
        return syn::Error::new_spanned(
            function.sig.output,
            "workflow functions must return anyhow::Result<Output>",
        )
        .to_compile_error();
    };
    let kind = if subworkflow {
        "subworkflow"
    } else {
        "workflow"
    };
    quote! {
        #function

        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, Default)]
        #visibility struct #runner_name;

        #[::lightflow::async_trait::async_trait]
        impl ::lightflow::workflow::Runnable<#input_ty, #output_ty> for #runner_name
        where
            #input_ty: Send + 'static,
            #output_ty: Send + 'static,
        {
            async fn run(&self, #input_name: #input_ty) -> ::lightflow::anyhow::Result<#output_ty> {
                #implementation_name(#input_name).await
            }
        }

        impl #runner_name {
            pub fn name(&self) -> &'static str {
                #workflow_name
            }

            pub fn kind(&self) -> &'static str {
                #kind
            }

            pub fn schema(&self) -> ::lightflow::serde_json::Value {
                ::lightflow::serde_json::json!({
                    "name": #workflow_name,
                    "kind": #kind,
                    "input": stringify!(#input_ty),
                    "output": stringify!(#output_ty)
                })
            }

            pub fn trace(&self) -> ::lightflow::serde_json::Value {
                ::lightflow::serde_json::json!({
                    "name": #workflow_name,
                    "kind": #kind
                })
            }
        }
    }
}

fn fn_arg_pattern(arg: &FnArg) -> Option<&Pat> {
    match arg {
        FnArg::Typed(typed) => Some(&typed.pat),
        FnArg::Receiver(_) => None,
    }
}

fn parse_workflow_input(function: &ItemFn) -> Option<(PatIdent, Type)> {
    if function.sig.inputs.len() != 1 {
        return None;
    }
    let FnArg::Typed(input) = function.sig.inputs.first()? else {
        return None;
    };
    let Pat::Ident(name) = input.pat.as_ref() else {
        return None;
    };
    Some((name.clone(), input.ty.as_ref().clone()))
}

fn parse_workflow_output(function: &ItemFn) -> Option<Type> {
    let ReturnType::Type(_, ty) = &function.sig.output else {
        return None;
    };
    result_inner_type(ty.as_ref())
}

fn result_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let first = args.args.first()?;
    let syn::GenericArgument::Type(ty) = first else {
        return None;
    };
    Some(ty.clone())
}
