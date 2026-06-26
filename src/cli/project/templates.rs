#[derive(Debug, Clone)]
pub(super) struct NodeTemplate {
    pub(super) runtime: Option<String>,
    source_body: String,
    skill_contract: String,
    example_args: Vec<String>,
    api_inputs: String,
}

impl NodeTemplate {
    pub(super) fn for_runtime(runtime: Option<&str>) -> Self {
        match runtime {
            Some("lightflow.image.generate") => Self::image_generate(),
            Some(runtime) => Self::runtime_placeholder(runtime),
            None => Self::passthrough(),
        }
    }

    pub(super) fn passthrough() -> Self {
        Self {
            runtime: None,
            source_body: [
                "        .input(\"value\", \"json\")",
                "        .input_description(\"value\", \"TODO: describe the input value.\")",
                "        .input_required(\"value\", true)",
                "        .input_widget(\"value\", \"json\")",
                "        .output(\"value\", \"json\")",
                "        .output_description(\"value\", \"TODO: describe the output value.\")",
            ]
            .join("\n"),
            skill_contract: [
                "- Input `value`: JSON value; required; widget `json`.",
                "- Output `value`: JSON value.",
                "- Define expected model requirements and runtime notes here.",
            ]
            .join("\n"),
            example_args: vec!["-i".to_owned(), "value='{\"hello\":\"world\"}'".to_owned()],
            api_inputs: "{\"value\":{\"hello\":\"world\"}}".to_owned(),
        }
    }

    fn image_generate() -> Self {
        Self {
            runtime: Some("lightflow.image.generate".to_owned()),
            source_body: [
                "        .input(\"prompt\", \"text\")",
                "        .input_description(\"prompt\", \"Positive text prompt used for image generation.\")",
                "        .input_required(\"prompt\", true)",
                "        .input_widget(\"prompt\", \"prompt\")",
                "        .input(\"negative\", \"text\")",
                "        .input_description(\"negative\", \"Optional negative prompt.\")",
                "        .input_required(\"negative\", false)",
                "        .input_default_json(\"negative\", \"\\\"\\\"\")",
                "        .input_widget(\"negative\", \"textarea\")",
                "        .input(\"width\", \"integer\")",
                "        .input_description(\"width\", \"Output image width in pixels.\")",
                "        .input_required(\"width\", false)",
                "        .input_default_json(\"width\", \"512\")",
                "        .input_range(\"width\", 64.0, 2048.0, 8.0)",
                "        .input_widget(\"width\", \"number\")",
                "        .input(\"height\", \"integer\")",
                "        .input_description(\"height\", \"Output image height in pixels.\")",
                "        .input_required(\"height\", false)",
                "        .input_default_json(\"height\", \"512\")",
                "        .input_range(\"height\", 64.0, 2048.0, 8.0)",
                "        .input_widget(\"height\", \"number\")",
                "        .input(\"seed\", \"integer\")",
                "        .input_description(\"seed\", \"Optional deterministic generation seed.\")",
                "        .input_required(\"seed\", false)",
                "        .input_widget(\"seed\", \"seed\")",
                "        .input(\"output_path\", \"path\")",
                "        .input_description(\"output_path\", \"Optional destination PNG path.\")",
                "        .input_required(\"output_path\", false)",
                "        .input_widget(\"output_path\", \"file_save\")",
                "        .input_artifact_kind(\"output_path\", \"image\")",
                "        .input(\"model\", \"text\")",
                "        .input_description(\"model\", \"Optional model variant id for the image_model requirement.\")",
                "        .input_required(\"model\", false)",
                "        .input_widget(\"model\", \"model_select\")",
                "        .input_model_requirement(\"model\", \"image_model\")",
                "        .output(\"image\", \"artifact\")",
                "        .output_description(\"image\", \"Generated image artifact metadata.\")",
                "        .output_artifact_kind(\"image\", \"image\")",
                "        .output_model_requirement(\"image\", \"image_model\")",
                "        .output(\"image_path\", \"path\")",
                "        .output_description(\"image_path\", \"Path to the generated PNG image.\")",
                "        .output_artifact_kind(\"image_path\", \"image\")",
                "        .output_model_requirement(\"image_path\", \"image_model\")",
                "        .runtime(\"image_runtime\", \"lightflow.image.generate\")",
                "        .model(\"image_model\", \"text-to-image\")",
            ]
            .join("\n"),
            skill_contract: [
                "- Runtime: `lightflow.image.generate`.",
                "- Input `prompt`: required positive prompt; widget `prompt`.",
                "- Input `negative`: optional negative prompt; default `\"\"`; widget `textarea`.",
                "- Input `width`: optional integer; default `512`; range `64..2048`; step `8`; widget `number`.",
                "- Input `height`: optional integer; default `512`; range `64..2048`; step `8`; widget `number`.",
                "- Input `seed`: optional integer seed; widget `seed`.",
                "- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.",
                "- Input `model`: optional model variant id bound to `image_model`; widget `model_select`.",
                "- Outputs: `image` artifact metadata and `image_path`; artifact kind `image`; bound to `image_model`.",
                "- Model requirement `image_model`: add concrete variants with `.hf_model(...)` before publishing.",
            ]
            .join("\n"),
            example_args: vec![
                "--prompt".to_owned(),
                "\"a quiet lake\"".to_owned(),
                "-i".to_owned(),
                "width=512".to_owned(),
                "-i".to_owned(),
                "height=512".to_owned(),
            ],
            api_inputs: "{\"prompt\":\"a quiet lake\",\"width\":512,\"height\":512}".to_owned(),
        }
    }

    fn runtime_placeholder(runtime: &str) -> Self {
        Self {
            runtime: Some(runtime.to_owned()),
            source_body: format!(
                "{}\n        .runtime(\"runtime\", {})",
                [
                    "        .input(\"value\", \"json\")",
                    "        .input_description(\"value\", \"TODO: describe the runtime input value.\")",
                    "        .input_required(\"value\", true)",
                    "        .input_widget(\"value\", \"json\")",
                    "        .output(\"value\", \"json\")",
                    "        .output_description(\"value\", \"TODO: describe the runtime output value.\")",
                ]
                .join("\n"),
                rust_string(runtime)
            ),
            skill_contract: format!(
                "- Runtime: `{runtime}`.\n- Input `value`: JSON value; required; widget `json`.\n- Output `value`: JSON value.\n- Add runtime-specific inputs, outputs, model requirements, and executor notes before publishing."
            ),
            example_args: vec!["-i".to_owned(), "value='{}'".to_owned()],
            api_inputs: "{\"value\":{}}".to_owned(),
        }
    }

    pub(super) fn example_command(&self, workflow_id: &str) -> Vec<String> {
        let mut command = vec!["lfw".to_owned(), "run".to_owned(), workflow_id.to_owned()];
        command.extend(self.example_args.iter().cloned());
        command
    }
}

pub(super) fn example_workflow_source(
    workflow_id: &str,
    name: &str,
    template: Option<&NodeTemplate>,
) -> String {
    let default_template;
    let template = match template {
        Some(template) => template,
        None => {
            default_template = NodeTemplate::passthrough();
            &default_template
        }
    };
    format!(
        "use lightflow::preload::*;\n\npub fn define() -> WorkflowSpec {{\n    workflow({})\n        .version(\"0.1.0\")\n        .name({})\n        .description(\"TODO: describe this workflow.\")\n{}\n        .build()\n}}\n",
        rust_string(workflow_id),
        rust_string(name),
        template.source_body
    )
}

pub(super) fn example_skill_source(
    name: &str,
    workflow_id: &str,
    template: Option<&NodeTemplate>,
) -> String {
    let default_template;
    let template = match template {
        Some(template) => template,
        None => {
            default_template = NodeTemplate::passthrough();
            &default_template
        }
    };
    let example = template.example_command(workflow_id).join(" ");
    let api_body = format!("{{\"inputs\":{}}}", template.api_inputs);
    format!(
        "---\nname: {}\ndescription: This skill should be used when working with the {} LightFlow workflow, configuring its inputs, running it through lfw, HTTP, or composing it with other LightFlow workflows.\nversion: 0.1.0\n---\n\n# {}\n\nUse this skill to understand the workflow contract for `{}`.\n\n## Workflow\n\n- Workflow id: `{}`\n{}\n\n## CLI Usage\n\n```bash\n{}\n```\n\n## API Usage\n\nStart `lfw serve`, then call the workflow through the shared HTTP run contract:\n\n```bash\ncurl -sS -X POST http://127.0.0.1:5174/workflows/{}/run \\\n  -H 'content-type: application/json' \\\n  -d '{}'\n```\n\nRun `lfw help {}` to inspect the generated Node Schema v1 contract.\n",
        rust_string(name),
        workflow_id,
        name,
        workflow_id,
        workflow_id,
        template.skill_contract,
        example,
        workflow_id,
        api_body,
        workflow_id
    )
}

pub(super) fn example_contract_test(workflow_id: &str, template: &NodeTemplate) -> String {
    example_contract_test_for_crate(workflow_id, &package_ident_from_id(workflow_id), template)
}

pub(super) fn example_contract_test_for_crate(
    workflow_id: &str,
    crate_ident: &str,
    template: &NodeTemplate,
) -> String {
    let runtime_assert = if let Some(runtime) = &template.runtime {
        format!(
            "    assert!(workflow.runtimes.iter().any(|runtime| runtime.capability == {}));\n",
            rust_string(runtime)
        )
    } else {
        "    assert!(workflow.runtimes.is_empty());\n".to_owned()
    };
    format!(
        "#[test]\nfn workflow_contract_is_valid() {{\n    let workflow = {}::define();\n    assert_eq!(workflow.id, {});\n    assert!(!workflow.inputs.is_empty());\n    assert!(!workflow.outputs.is_empty());\n{}    assert!(workflow.inputs.iter().all(|port| !port.name.is_empty() && !port.ty.is_empty()));\n    assert!(workflow.outputs.iter().all(|port| !port.name.is_empty() && !port.ty.is_empty()));\n}}\n",
        crate_ident,
        rust_string(workflow_id),
        runtime_assert
    )
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}

pub(super) fn package_name_from_id(id: &str) -> String {
    let mut name = String::new();
    let mut previous_dash = false;
    for character in id.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            name.push('-');
            previous_dash = true;
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "workflow".to_owned()
    } else {
        name.to_owned()
    }
}

pub(super) fn package_ident_from_id(id: &str) -> String {
    package_name_from_id(id).replace('-', "_")
}

pub(super) fn workflow_skill_name(id: &str) -> String {
    package_name_from_id(id)
}

pub(super) fn title_from_id(id: &str) -> String {
    let suffix = id.rsplit('.').next().unwrap_or(id);
    suffix
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
