#[derive(Debug, Clone)]
pub(super) struct NodeTemplate {
    pub(super) runtime: Option<String>,
    port_source: String,
    runtime_source: Option<String>,
    skill_contract: String,
    example_args: Vec<String>,
    api_inputs: Option<String>,
}

impl NodeTemplate {
    pub(super) fn for_runtime(runtime: Option<&str>) -> Self {
        match runtime {
            Some("lightflow.comfyui.workflow") => Self::comfyui_workflow(),
            Some("lightflow.image.generate") => Self::image_generate(),
            Some(runtime) => Self::runtime_placeholder(runtime),
            None => Self::passthrough(),
        }
    }

    pub(super) fn passthrough() -> Self {
        Self {
            runtime: None,
            port_source: r#"        input "value": "json" {
            description: "TODO: describe the input value.",
            required: true,
            widget: "json",
        }
        output "value": "json" {
            description: "TODO: describe the output value.",
        }"#
            .to_owned(),
            runtime_source: None,
            skill_contract: [
                "- Input `value`: JSON value; required; widget `json`.",
                "- Output `value`: JSON value.",
                "- Define expected model requirements and runtime notes here.",
            ]
            .join("\n"),
            example_args: vec!["-i".to_owned(), "value='{\"hello\":\"world\"}'".to_owned()],
            api_inputs: Some("{\"value\":{\"hello\":\"world\"}}".to_owned()),
        }
    }

    fn image_generate() -> Self {
        Self {
            runtime: Some("lightflow.image.generate".to_owned()),
            port_source: r#"        input "prompt": "text" {
            description: "Positive text prompt used for image generation.",
            required: true,
            widget: "prompt",
        }
        input "negative": "text" {
            description: "Optional negative prompt.",
            required: false,
            default: "",
            widget: "textarea",
        }
        input "width": "integer" {
            description: "Output image width in pixels.",
            required: false,
            default: 512,
            range: [64.0, 2048.0, 8.0],
            widget: "number",
        }
        input "height": "integer" {
            description: "Output image height in pixels.",
            required: false,
            default: 512,
            range: [64.0, 2048.0, 8.0],
            widget: "number",
        }
        input "seed": "integer" {
            description: "Optional deterministic generation seed.",
            required: false,
            widget: "seed",
        }
        input "output_path": "path" {
            description: "Optional destination PNG path.",
            required: false,
            widget: "file_save",
            artifact: "image",
        }
        output "image": "artifact" {
            description: "Generated image artifact metadata.",
            artifact: "image",
        }
        output "image_path": "path" {
            description: "Path to the generated PNG image.",
            artifact: "image",
        }"#
                .to_owned(),
            runtime_source: Some(
                "        .builtin_runtime(\"image_runtime\", \"lightflow.image.generate\", \"builtin.preview.v1\")"
                    .to_owned(),
            ),
            skill_contract: [
                "- Runtime: `lightflow.image.generate`.",
                "- Engine: `builtin.preview.v1`.",
                "- This is a deterministic preview that only validates the pipeline; it does not represent production model quality.",
                "- To use a real model backend, replace the preview runtime with that backend's contract and declare its concrete model requirements.",
                "- Input `prompt`: required positive prompt; widget `prompt`.",
                "- Input `negative`: optional negative prompt; default `\"\"`; widget `textarea`.",
                "- Input `width`: optional integer; default `512`; range `64..2048`; step `8`; widget `number`.",
                "- Input `height`: optional integer; default `512`; range `64..2048`; step `8`; widget `number`.",
                "- Input `seed`: optional integer seed; widget `seed`.",
                "- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.",
                "- Outputs: `image` artifact metadata and `image_path`; artifact kind `image`.",
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
            api_inputs: Some(
                "{\"prompt\":\"a quiet lake\",\"width\":512,\"height\":512}".to_owned(),
            ),
        }
    }

    fn comfyui_workflow() -> Self {
        Self {
            runtime: Some("lightflow.comfyui.workflow".to_owned()),
            port_source: r#"        input "workflow": "json" {
            description: "Required inline ComfyUI Save (API Format) prompt graph.",
            required: true,
            widget: "json",
        }
        input "node_inputs": "json" {
            description: "Optional node-id to input-name overrides applied before upload bindings.",
            required: false,
            default: {},
            widget: "json",
        }
        input "uploads": "json" {
            description: "Optional images or masks uploaded and bound to ComfyUI node inputs.",
            required: false,
            default: [],
            widget: "json",
        }
        input "server_url": "text" {
            description: "Optional ComfyUI HTTP base URL; LIGHTFLOW_COMFYUI_URL or localhost:8188 is used otherwise.",
            required: false,
            widget: "text",
        }
        input "extra_data": "json" {
            description: "Optional ComfyUI prompt extra_data object.",
            required: false,
            widget: "json",
        }
        input "client_id": "text" {
            description: "Optional ComfyUI client id sent with the prompt.",
            required: false,
            widget: "text",
        }
        input "output_node_ids": "json" {
            description: "Optional list of top-level ComfyUI output node ids to download.",
            required: false,
            widget: "json",
        }
        input "output_dir": "path" {
            description: "Optional local artifact directory relative to the LightFlow repository.",
            required: false,
            widget: "directory",
        }
        input "timeout_ms": "integer" {
            description: "Total ComfyUI execution timeout in milliseconds.",
            required: false,
            default: 300000,
            widget: "number",
        }
        input "poll_interval_ms": "integer" {
            description: "History polling interval in milliseconds.",
            required: false,
            default: 250,
            widget: "number",
        }
        output "prompt_id": "text" { description: "ComfyUI prompt id.", }
        output "artifacts": "json" { description: "Downloaded file artifact records.", }
        output "files": "json" { description: "Alias of downloaded file artifact records.", }
        output "image": "artifact" {
            description: "First downloaded still-image artifact, or null.",
            artifact: "image",
        }
        output "image_path": "path" {
            description: "Path to the first downloaded still image, or null.",
            artifact: "image",
        }
        output "history": "json" { description: "Completed ComfyUI history entry.", }
        output "remote_outputs": "json" { description: "All remote ComfyUI node outputs, including non-file values.", }
        output "submitted_workflow": "json" { description: "Resolved API graph submitted after overrides and upload bindings.", }
        output "workflow_sha256": "text" { description: "SHA-256 of the resolved submitted workflow.", }
        output "upload_fingerprints": "json" { description: "Stable uploaded content hashes and remote targets.", }"#
                .to_owned(),
            runtime_source: Some(
                "        .builtin_runtime(\"comfyui_runtime\", \"lightflow.comfyui.workflow\", \"comfyui.api.v1\")"
                    .to_owned(),
            ),
            skill_contract: r#"- Runtime: `lightflow.comfyui.workflow`; engine: `comfyui.api.v1`.
- `workflow` must be an inline ComfyUI Save (API Format) prompt graph, not the UI workflow JSON.
- `node_inputs` can override prompt, seed, steps, cfg, model, control, or any other node input; every node id must come from your complete exported graph.
- `uploads` sends images or masks to `/upload/image`; each `bind` writes the returned ComfyUI reference into a node input.
- LightFlow submits `/prompt`, polls `/history/<prompt_id>`, and downloads every file descriptor through `/view`.
- ComfyUI owns model installation, custom nodes, VRAM policy, and model quality. Executor availability only means this build can make the API calls; the endpoint is checked at run.
- Optional Authorization comes only from `LIGHTFLOW_COMFYUI_AUTHORIZATION` and is never recorded.

## comfy-run.json shape only

Shape only: replace `workflow` with a complete Save (API Format) export before running. The node ids below must be replaced with ids from that complete graph.

```json
{"workflow":{"<complete-api-format-graph>":"REPLACE_ME"},"node_inputs":{"<node-id-from-your-complete-graph>":{"seed":42}}}
```

## Upload binding fragment

Merge this fragment into the same complete run object. Upload binding node ids must identify actual image or mask inputs in your graph.

```json
{"uploads":[{"path":"input.png","bind":[{"node_id":"<load-image-node-id-from-your-complete-graph>","input":"image"}]},{"path":"mask.png","type":"temp","bind":[{"node_id":"<mask-node-id-from-your-complete-graph>","input":"image"}]}]}
```"#
                .to_owned(),
            example_args: vec![
                "--inputs".to_owned(),
                "@comfy-run.json".to_owned(),
            ],
            api_inputs: None,
        }
    }

    fn runtime_placeholder(runtime: &str) -> Self {
        Self {
            runtime: Some(runtime.to_owned()),
            port_source: r#"        input "value": "json" {
            description: "TODO: describe the runtime input value.",
            required: true,
            widget: "json",
        }
        output "value": "json" {
            description: "TODO: describe the runtime output value.",
        }"#
            .to_owned(),
            runtime_source: Some(format!(
                "        .runtime(\"runtime\", {})",
                rust_string(runtime)
            )),
            skill_contract: format!(
                "- Runtime: `{runtime}`.\n- Input `value`: JSON value; required; widget `json`.\n- Output `value`: JSON value.\n- Add runtime-specific inputs, outputs, model requirements, and executor notes before publishing."
            ),
            example_args: vec!["-i".to_owned(), "value='{}'".to_owned()],
            api_inputs: Some("{\"value\":{}}".to_owned()),
        }
    }

    pub(super) fn example_command(&self, workflow_id: &str) -> Vec<String> {
        let mut command = vec!["lfw".to_owned(), "run".to_owned(), workflow_id.to_owned()];
        command.extend(self.example_args.iter().cloned());
        command
    }
}

pub(super) fn example_workflow_source(
    _workflow_id: &str,
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
    let runtime = template
        .runtime_source
        .as_deref()
        .map(|source| format!("\n{source}"))
        .unwrap_or_default();
    format!(
        "use lightflow::preload::*;\n\npub fn define() -> WorkflowSpec {{\n    workflow! {{\n{}\n    }}\n        .name({})\n        .description(\"TODO: describe this workflow.\"){}\n        .build()\n}}\n",
        template.port_source,
        rust_string(name),
        runtime
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
    let api_body = format!(
        "{{\"inputs\":{}}}",
        template.api_inputs.as_deref().unwrap_or("{}")
    );
    let mut skill = format!(
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
    );
    if template.api_inputs.is_none() {
        let Some(start) = skill.find("Start `lfw serve`") else {
            return skill;
        };
        let Some(offset) = skill[start..].find("\n\nRun `lfw help") else {
            return skill;
        };
        let end = start + offset;
        let api_usage = format!(
            "Create `comfy-http-request.json` with `{{\"inputs\": <complete run object from comfy-run.json>}}`. The nested `workflow` must be a complete ComfyUI Save (API Format) export. Then send the file without embedding a partial graph:\n\n```bash\ncurl -sS -X POST http://127.0.0.1:5174/workflows/{workflow_id}/run \\\n+  -H 'content-type: application/json' \\\n+  --data-binary @comfy-http-request.json\n```"
        )
        .replace("\n+  ", "\n  ");
        skill.replace_range(start..end, &api_usage);
    }
    skill
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
