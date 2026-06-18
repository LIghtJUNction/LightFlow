use super::project::validate_spec_id;
use super::{CliError, CliResult, required_flag_value};
use crate::api::ApiService;
use crate::workflow::WorkflowSummary;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum ListMode {
    Brief,
    Detail,
}

pub(super) struct ListOptions {
    pub(super) mode: ListMode,
    pub(super) category: Option<String>,
    pub(super) categories: bool,
}

#[derive(Serialize)]
struct BriefWorkflowRow {
    id: String,
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
}

#[derive(Serialize)]
struct WorkflowCategoryRow {
    category: String,
    workflows: usize,
}

pub(super) fn parse_list_options(args: &[String]) -> CliResult<ListOptions> {
    let mut mode = ListMode::Brief;
    let mut category = None;
    let mut categories = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--brief" | "--short" => mode = ListMode::Brief,
            "--detail" | "--detailed" | "-l" => mode = ListMode::Detail,
            "--category" => {
                if category.is_some() {
                    return Err(CliError::Usage("duplicate flag --category".to_owned()));
                }
                let value = required_flag_value(args, index, "--category")?;
                validate_spec_id(value, "workflow category")?;
                category = Some(value.to_owned());
                index += 1;
            }
            "--categories" => categories = true,
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for list: {}",
                    args[index]
                )));
            }
        }
        index += 1;
    }
    if categories && category.is_some() {
        return Err(CliError::Usage(
            "--categories cannot be combined with --category".to_owned(),
        ));
    }
    Ok(ListOptions {
        mode,
        category,
        categories,
    })
}

pub(super) fn list_workflows(
    service: &ApiService,
    options: &ListOptions,
) -> CliResult<serde_json::Value> {
    let summaries = filtered_workflow_summaries(service, options.category.as_deref())?;
    if options.categories {
        let mut counts = BTreeMap::new();
        for summary in summaries {
            let category = summary
                .category
                .unwrap_or_else(|| "uncategorized".to_owned());
            *counts.entry(category).or_insert(0usize) += 1;
        }
        let categories = counts
            .into_iter()
            .map(|(category, workflows)| WorkflowCategoryRow {
                category,
                workflows,
            })
            .collect::<Vec<_>>();
        return Ok(json!({ "categories": categories }));
    }

    match options.mode {
        ListMode::Brief => {
            let workflows = summaries
                .into_iter()
                .map(|summary| BriefWorkflowRow {
                    id: summary.id,
                    name: summary.name,
                    version: summary.version,
                    category: summary.category,
                })
                .collect::<Vec<_>>();
            Ok(json!({ "workflows": workflows }))
        }
        ListMode::Detail => {
            let workflows = summaries
                .into_iter()
                .map(|summary| service.get_workflow(&summary.id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({ "workflows": workflows }))
        }
    }
}

fn filtered_workflow_summaries(
    service: &ApiService,
    category: Option<&str>,
) -> CliResult<Vec<WorkflowSummary>> {
    let workflows = service
        .list_workflows()?
        .workflows
        .into_iter()
        .filter(|summary| category_matches(summary.category.as_deref(), category))
        .collect();
    Ok(workflows)
}

fn category_matches(actual: Option<&str>, expected: Option<&str>) -> bool {
    match expected {
        None => true,
        Some("uncategorized") => actual.is_none(),
        Some(expected) => actual == Some(expected),
    }
}
