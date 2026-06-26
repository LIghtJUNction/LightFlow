use super::{CliError, CliResult};
use std::env;
use std::path::PathBuf;

mod scaffold;
mod templates;

pub(super) use scaffold::{
    add_workflow, init_plugin_project, init_workflow_project, normalize_workflow_id,
    validate_spec_id, workflow_collection_manifest, workflow_crate_dir_name, workspace_manifest,
};

#[derive(Clone, Copy)]
pub(super) enum InitMode {
    Workflow,
    Plugin,
}

pub(super) struct InitOptions {
    pub(super) mode: InitMode,
    pub(super) root: PathBuf,
}

pub(super) fn parse_init_options(args: &[String]) -> CliResult<InitOptions> {
    let mut mode = InitMode::Workflow;
    let mut root = None;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" | "help" => return Err(CliError::Usage(init_usage())),
            "--workflow" => {
                if matches!(mode, InitMode::Plugin) {
                    return Err(CliError::Usage(
                        "--workflow cannot be combined with --plugin".to_owned(),
                    ));
                }
                mode = InitMode::Workflow;
            }
            "--plugin" => {
                if matches!(mode, InitMode::Plugin) {
                    return Err(CliError::Usage("duplicate flag --plugin".to_owned()));
                }
                mode = InitMode::Plugin;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(init_usage()));
            }
            value => {
                if root.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for init: {value}"
                    )));
                }
                root = Some(PathBuf::from(value));
            }
        }
    }
    Ok(InitOptions {
        mode,
        root: root.unwrap_or(env::current_dir()?),
    })
}

fn init_usage() -> String {
    [
        "usage:",
        "  lfw init [--workflow|--plugin] [path]",
        "",
        "Initializes a LightFlow workspace at path or the current directory.",
        "--workflow creates a workflow collection with Cargo workspace metadata and workflows/.",
        "--plugin creates a single Cargo crate that can expose one workflow from src/lib.rs.",
        "Workflow initialization also prepares the default LightFlow home, .lfwrc, and shell source line used for global workflow discovery.",
    ]
    .join("\n")
}

pub(super) struct AddWorkflowOptions {
    pub(super) workflow_id: String,
    pub(super) name: Option<String>,
    pub(super) category: Option<String>,
    pub(super) runtime: Option<String>,
    pub(super) global: bool,
}

pub(super) fn parse_add_workflow_options(args: &[String]) -> CliResult<AddWorkflowOptions> {
    let mut workflow_id = None;
    let mut name = None;
    let mut category = None;
    let mut runtime = None;
    let mut global = false;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "-h" | "--help" | "help" => return Err(CliError::Usage(new_usage())),
            "--global" | "-g" => {
                global = true;
                index += 1;
                continue;
            }
            "--name" => {
                if name.is_some() {
                    return Err(CliError::Usage("duplicate flag --name".to_owned()));
                }
                name = Some(required_new_flag_value(args, index)?.to_owned());
            }
            "--category" => {
                if category.is_some() {
                    return Err(CliError::Usage("duplicate flag --category".to_owned()));
                }
                let value = required_new_flag_value(args, index)?;
                validate_spec_id(value, "workflow category")?;
                category = Some(value.to_owned());
            }
            "--runtime" => {
                if runtime.is_some() {
                    return Err(CliError::Usage("duplicate flag --runtime".to_owned()));
                }
                runtime = Some(required_new_flag_value(args, index)?.to_owned());
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(new_usage()));
            }
            value => {
                if workflow_id.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for new: {value}"
                    )));
                }
                workflow_id = Some(value.to_owned());
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    let Some(workflow_id) = workflow_id else {
        return Err(CliError::Usage(new_usage()));
    };
    Ok(AddWorkflowOptions {
        workflow_id,
        name,
        category,
        runtime,
        global,
    })
}

fn required_new_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(new_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(new_usage()));
    }
    Ok(value)
}

fn new_usage() -> String {
    [
        "usage:",
        "  lfw new <workflow_id> --category <name> [--name <name>] [--runtime <capability>] [--global|-g]",
        "",
        "Creates a workflow crate with starter source, Cargo metadata, and a colocated agent skill.",
        "--category selects the workflows/<category>/<crate> directory and is required.",
        "--runtime selects a runtime-aware template, such as lightflow.image.generate.",
        "--global creates the workflow under the default LightFlow home workspace instead of the current project.",
    ]
    .join("\n")
}
