use super::{ApiError, ApiResult};
use crate::workflow::PortSpec;
use std::path::Path;

pub(super) fn string_arg(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    index: usize,
    method: &str,
    path: &Path,
) -> ApiResult<String> {
    let Some(argument) = args.iter().nth(index) else {
        return Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) in {:?} is missing argument {}",
            path,
            index + 1
        )));
    };
    match argument {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) argument {} in {:?} must be a string literal",
            index + 1,
            path
        ))),
    }
}

pub(super) fn json_string_arg(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    index: usize,
    method: &str,
    path: &Path,
) -> ApiResult<serde_json::Value> {
    let raw = string_arg(args, index, method, path)?;
    serde_json::from_str(&raw).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) argument {} in {:?} must be JSON: {error}",
            index + 1,
            path
        ))
    })
}

pub(super) fn json_array_string_arg(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    index: usize,
    method: &str,
    path: &Path,
) -> ApiResult<Vec<serde_json::Value>> {
    let value = json_string_arg(args, index, method, path)?;
    match value {
        serde_json::Value::Array(values) => Ok(values),
        _ => Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) argument {} in {:?} must be a JSON array",
            index + 1,
            path
        ))),
    }
}

pub(super) fn number_arg(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    index: usize,
    method: &str,
    path: &Path,
) -> ApiResult<f64> {
    let Some(argument) = args.iter().nth(index) else {
        return Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) in {:?} is missing argument {}",
            path,
            index + 1
        )));
    };
    match argument {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Float(value),
            ..
        }) => value.base10_parse::<f64>().map_err(|error| {
            ApiError::InvalidRequest(format!(
                "workflow builder .{method}(...) argument {} in {:?} must be a number: {error}",
                index + 1,
                path
            ))
        }),
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(value),
            ..
        }) => value.base10_parse::<f64>().map_err(|error| {
            ApiError::InvalidRequest(format!(
                "workflow builder .{method}(...) argument {} in {:?} must be a number: {error}",
                index + 1,
                path
            ))
        }),
        _ => Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) argument {} in {:?} must be a number literal",
            index + 1,
            path
        ))),
    }
}

pub(super) fn bool_arg(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    index: usize,
    method: &str,
    path: &Path,
) -> ApiResult<bool> {
    let Some(argument) = args.iter().nth(index) else {
        return Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) in {:?} is missing argument {}",
            path,
            index + 1
        )));
    };
    match argument {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Bool(value),
            ..
        }) => Ok(value.value),
        _ => Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) argument {} in {:?} must be a boolean literal",
            index + 1,
            path
        ))),
    }
}

pub(super) fn find_port_mut<'a>(ports: &'a mut [PortSpec], name: &str) -> Option<&'a mut PortSpec> {
    ports.iter_mut().find(|port| port.name == name)
}

pub(super) fn expect_arg_len(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    expected: usize,
    method: &str,
    path: &Path,
) -> ApiResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) in {:?} expects {expected} arguments, got {}",
            path,
            args.len()
        )))
    }
}
