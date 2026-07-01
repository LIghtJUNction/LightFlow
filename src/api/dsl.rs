use super::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;
use std::fs;
use std::path::Path;

mod arguments;
mod builder;
use builder::{define_expression, parse_workflow_builder};

pub(super) fn read_optional_workflow_source(path: &Path) -> ApiResult<Option<WorkflowSpec>> {
    let source = fs::read_to_string(path).map_err(ApiError::from)?;
    let file = syn::parse_file(&source).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Rust workflow source in {:?}: {error}",
            path
        ))
    })?;
    let Some(define) = file.items.iter().find_map(|item| match item {
        syn::Item::Fn(function) if function.sig.ident == "define" => Some(function),
        _ => None,
    }) else {
        return Ok(None);
    };
    let expression = define_expression(define).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow source {:?} must return a workflow(...) builder expression",
            path
        ))
    })?;
    parse_workflow_builder(expression, path).map(Some)
}

pub(crate) fn read_workflow_source(path: &Path) -> ApiResult<WorkflowSpec> {
    let source = fs::read_to_string(path).map_err(ApiError::from)?;
    let file = syn::parse_file(&source).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Rust workflow source in {:?}: {error}",
            path
        ))
    })?;
    let define = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "define" => Some(function),
            _ => None,
        })
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "workflow source {:?} must define pub fn define() -> WorkflowSpec",
                path
            ))
        })?;
    let expression = define_expression(define).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow source {:?} must return a workflow(...) builder expression",
            path
        ))
    })?;
    parse_workflow_builder(expression, path)
}
