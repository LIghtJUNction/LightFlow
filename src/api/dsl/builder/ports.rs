use super::super::arguments::{
    bool_arg, expect_arg_len, find_port_mut, json_array_string_arg, json_string_arg, number_arg,
    string_arg,
};
use crate::api::ApiResult;
use crate::workflow::{PortSpec, WorkflowSpec};
use std::path::Path;

pub(super) fn apply_port_builder_method(
    workflow: &mut WorkflowSpec,
    method: &str,
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    path: &Path,
) -> ApiResult<bool> {
    match method {
        "input" => {
            workflow.inputs.push(PortSpec::new(
                string_arg(args, 0, method, path)?,
                string_arg(args, 1, method, path)?,
            ));
            expect_arg_len(args, 2, method, path)?;
        }
        "output" => {
            workflow.outputs.push(PortSpec::new(
                string_arg(args, 0, method, path)?,
                string_arg(args, 1, method, path)?,
            ));
            expect_arg_len(args, 2, method, path)?;
        }
        "input_description" => {
            let name = string_arg(args, 0, method, path)?;
            let description = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.description = Some(description);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "output_description" => {
            let name = string_arg(args, 0, method, path)?;
            let description = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.outputs, &name) {
                port.description = Some(description);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "input_required" => {
            let name = string_arg(args, 0, method, path)?;
            let required = bool_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.required = Some(required);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "input_default_json" => {
            let name = string_arg(args, 0, method, path)?;
            let value = json_string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.default = Some(value);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "input_range" => {
            let name = string_arg(args, 0, method, path)?;
            let min = number_arg(args, 1, method, path)?;
            let max = number_arg(args, 2, method, path)?;
            let step = number_arg(args, 3, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.min = Some(min);
                port.max = Some(max);
                port.step = Some(step);
            }
            expect_arg_len(args, 4, method, path)?;
        }
        "input_enum_json" => {
            let name = string_arg(args, 0, method, path)?;
            let values = json_array_string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.enum_values = values;
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "input_widget" => {
            let name = string_arg(args, 0, method, path)?;
            let widget = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.widget = Some(widget);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "input_artifact_kind" => {
            let name = string_arg(args, 0, method, path)?;
            let kind = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.artifact_kind = Some(kind);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "output_artifact_kind" => {
            let name = string_arg(args, 0, method, path)?;
            let kind = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.outputs, &name) {
                port.artifact_kind = Some(kind);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "input_model_requirement" => {
            let name = string_arg(args, 0, method, path)?;
            let requirement_id = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                port.model_requirement = Some(requirement_id);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        "output_model_requirement" => {
            let name = string_arg(args, 0, method, path)?;
            let requirement_id = string_arg(args, 1, method, path)?;
            if let Some(port) = find_port_mut(&mut workflow.outputs, &name) {
                port.model_requirement = Some(requirement_id);
            }
            expect_arg_len(args, 2, method, path)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
