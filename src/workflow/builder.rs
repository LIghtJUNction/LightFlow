use super::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    RuntimeRequirement, WorkflowCondition, WorkflowEdge, WorkflowEndpoint, WorkflowNode,
    WorkflowNodeKind, WorkflowPosition, WorkflowSpec,
};

mod ports;

/// Builder used by source-controlled Rust workflow files.
#[derive(Debug, Clone)]
pub struct WorkflowBuilder {
    pub(super) spec: WorkflowSpec,
}

impl WorkflowBuilder {
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.spec.name = name.into();
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.spec.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.spec.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn depends_on(
        mut self,
        workflow_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.register_dependency(workflow_id.into(), Some(version.into()), None);
        self
    }

    #[must_use]
    pub fn depends_on_crate(
        mut self,
        workflow_id: impl Into<String>,
        version: impl Into<String>,
        crate_name: impl Into<String>,
    ) -> Self {
        let version = version.into();
        self.register_dependency(
            workflow_id.into(),
            Some(version.clone()),
            Some(CargoDependency {
                crate_name: crate_name.into(),
                version: Some(version),
                source: None,
                package: None,
            }),
        );
        self
    }

    #[must_use]
    pub fn depends_on_path(
        mut self,
        workflow_id: impl Into<String>,
        version: impl Into<String>,
        crate_name: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        let version = version.into();
        self.register_dependency(
            workflow_id.into(),
            Some(version.clone()),
            Some(CargoDependency {
                crate_name: crate_name.into(),
                version: Some(version),
                source: Some(CargoDependencySource::Path(path.into())),
                package: None,
            }),
        );
        self
    }

    #[must_use]
    pub fn depends_on_git(
        mut self,
        workflow_id: impl Into<String>,
        version: impl Into<String>,
        crate_name: impl Into<String>,
        git: impl Into<String>,
        package: impl Into<String>,
    ) -> Self {
        let version = version.into();
        let package = package.into();
        self.register_dependency(
            workflow_id.into(),
            Some(version.clone()),
            Some(CargoDependency {
                crate_name: crate_name.into(),
                version: Some(version),
                source: Some(CargoDependencySource::Git(git.into())),
                package: Some(package).filter(|package| !package.is_empty()),
            }),
        );
        self
    }

    #[must_use]
    pub fn model(mut self, id: impl Into<String>, capability: impl Into<String>) -> Self {
        self.spec.models.push(ModelRequirement {
            id: id.into(),
            capability: capability.into(),
            variants: Vec::new(),
        });
        self
    }

    #[must_use]
    pub fn hf_model(
        mut self,
        requirement_id: impl Into<String>,
        variant_id: impl Into<String>,
        capability: impl Into<String>,
        format: impl Into<String>,
        repo: impl Into<String>,
        file: impl Into<String>,
    ) -> Self {
        let requirement_id = requirement_id.into();
        let capability = capability.into();
        let variant = ModelVariant {
            id: variant_id.into(),
            provider: ModelProvider::HuggingFace,
            format: format.into(),
            repo: repo.into(),
            file: Some(file.into()).filter(|file| !file.is_empty()),
        };

        if let Some(requirement) = self
            .spec
            .models
            .iter_mut()
            .find(|requirement| requirement.id == requirement_id)
        {
            requirement.variants.push(variant);
        } else {
            self.spec.models.push(ModelRequirement {
                id: requirement_id,
                capability,
                variants: vec![variant],
            });
        }
        self
    }

    #[must_use]
    pub fn runtime(mut self, id: impl Into<String>, capability: impl Into<String>) -> Self {
        self.spec.runtimes.push(RuntimeRequirement {
            id: id.into(),
            capability: capability.into(),
            engine: None,
        });
        self
    }

    #[must_use]
    pub fn builtin_runtime(
        mut self,
        id: impl Into<String>,
        capability: impl Into<String>,
        engine: impl Into<String>,
    ) -> Self {
        self.spec.runtimes.push(RuntimeRequirement {
            id: id.into(),
            capability: capability.into(),
            engine: Some(engine.into()),
        });
        self
    }

    /// Instantiates a nested workflow node and registers an unversioned dependency.
    ///
    /// Nested workflows are already discoverable from the project catalog; the
    /// dependency version follows whatever Cargo/package version is installed.
    /// Use [`Self::node_version`] when the composite workflow should pin a version.
    #[must_use]
    pub fn node(mut self, id: impl Into<String>, workflow_id: impl Into<String>) -> Self {
        let workflow_id = workflow_id.into();
        self.push_workflow_node(id, workflow_id.clone(), false);
        self.register_dependency(workflow_id, None, None);
        self
    }

    /// Instantiates a nested workflow node and pins a dependency version.
    #[must_use]
    pub fn node_version(
        mut self,
        id: impl Into<String>,
        workflow_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        let workflow_id = workflow_id.into();
        let version = version.into();
        self.push_workflow_node(id, workflow_id.clone(), false);
        self.register_dependency(workflow_id, Some(version), None);
        self
    }

    #[must_use]
    pub fn disabled_node(mut self, id: impl Into<String>, workflow_id: impl Into<String>) -> Self {
        let workflow_id = workflow_id.into();
        self.push_workflow_node(id, workflow_id.clone(), true);
        self.register_dependency(workflow_id, None, None);
        self
    }

    #[must_use]
    pub fn if_node(
        mut self,
        id: impl Into<String>,
        input: impl Into<String>,
        expected: bool,
        then_workflow_id: impl Into<String>,
        else_workflow_id: impl Into<String>,
    ) -> Self {
        let then_workflow_id = then_workflow_id.into();
        let else_workflow_id = else_workflow_id.into();
        self.spec.nodes.push(WorkflowNode {
            id: id.into(),
            kind: WorkflowNodeKind::If,
            workflow_id: String::new(),
            condition: Some(WorkflowCondition::InputEquals {
                input: input.into(),
                value: serde_json::Value::Bool(expected),
            }),
            then_workflow_id: Some(then_workflow_id.clone()),
            else_workflow_id: Some(else_workflow_id.clone()),
            title: None,
            disabled: false,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        });
        self.register_dependency(then_workflow_id, None, None);
        self.register_dependency(else_workflow_id, None, None);
        self
    }

    #[must_use]
    pub fn edge(
        mut self,
        from_node: impl Into<String>,
        from_port: impl Into<String>,
        to_node: impl Into<String>,
        to_port: impl Into<String>,
    ) -> Self {
        self.spec.edges.push(WorkflowEdge {
            from: WorkflowEndpoint {
                node: from_node.into(),
                port: from_port.into(),
            },
            to: WorkflowEndpoint {
                node: to_node.into(),
                port: to_port.into(),
            },
        });
        self
    }

    /// Connects `from_node.from_port` to `to_node.to_port` using dotted endpoints.
    #[must_use]
    pub fn wire(self, from: impl AsRef<str>, to: impl AsRef<str>) -> Self {
        let (from_node, from_port) = split_endpoint(from.as_ref());
        let (to_node, to_port) = split_endpoint(to.as_ref());
        self.edge(from_node, from_port, to_node, to_port)
    }

    #[must_use]
    pub fn build(self) -> WorkflowSpec {
        self.spec
    }

    fn push_workflow_node(
        &mut self,
        id: impl Into<String>,
        workflow_id: impl Into<String>,
        disabled: bool,
    ) {
        self.spec.nodes.push(WorkflowNode {
            id: id.into(),
            kind: WorkflowNodeKind::Workflow,
            workflow_id: workflow_id.into(),
            condition: None,
            then_workflow_id: None,
            else_workflow_id: None,
            title: None,
            disabled,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        });
    }

    fn register_dependency(
        &mut self,
        workflow_id: String,
        version: Option<String>,
        install: Option<CargoDependency>,
    ) {
        self.spec.merge_dependency(workflow_id, version, install);
    }
}

fn split_endpoint(value: &str) -> (String, String) {
    let (node, port) = value
        .split_once('.')
        .unwrap_or_else(|| panic!("invalid endpoint {value:?}: expected `<node>.<port>`"));
    assert!(
        !node.is_empty() && !port.is_empty(),
        "invalid endpoint {value:?}: node and port must both be non-empty"
    );
    (node.to_owned(), port.to_owned())
}

impl From<WorkflowBuilder> for WorkflowSpec {
    fn from(builder: WorkflowBuilder) -> Self {
        builder.spec
    }
}

#[cfg(test)]
mod tests {
    use crate::workflow;
    use crate::workflow::workflow_with_identity;

    #[test]
    fn category_sets_optional_workflow_metadata() {
        let workflow = workflow_with_identity("lightflow.example", "0.1.0")
            .category("media")
            .build();

        assert_eq!(workflow.category.as_deref(), Some("media"));
    }

    #[test]
    fn workflow_macro_builds_ports_with_native_metadata() {
        let workflow = workflow! {
            output "image": "artifact" {
                description: "Generated image.",
                artifact: "image",
                model: "image_model",
            }
            input "condition": "boolean" {
                description: "Whether to render.",
                required: true,
                default: false,
                widget: "checkbox",
            }
            input "strength": "number" {
                range: [0.0, 1.0, 0.05],
                choices: [0.25, 0.5, 0.75, 1.0],
            }
            input "images": "artifact[]"
            input "config": "json" {
                default: {
                    "enabled": true,
                    "items": [null, false, {"offset": -2,},],
                },
            }
            input "offset": "integer" {
                default: -1,
                range: [-10, 10, 1],
            }
        }
        .name("Macro ports")
        .build();

        assert_eq!(workflow.inputs.len(), 5);
        assert_eq!(workflow.outputs.len(), 1);
        assert_eq!(workflow.inputs[0].default, Some(serde_json::json!(false)));
        assert_eq!(
            workflow.inputs[1].enum_values,
            serde_json::json!([0.25, 0.5, 0.75, 1.0])
                .as_array()
                .expect("choices")
                .clone()
        );
        assert_eq!(
            workflow.outputs[0].model_requirement.as_deref(),
            Some("image_model")
        );
        assert_eq!(workflow.inputs[2].ty, "artifact[]");
        assert_eq!(
            workflow.inputs[3].default,
            Some(serde_json::json!({
                "enabled": true,
                "items": [null, false, {"offset": -2}],
            }))
        );
        assert_eq!(workflow.inputs[4].default, Some(serde_json::json!(-1)));
        assert_eq!(workflow.inputs[4].min, Some(-10.0));
    }

    #[test]
    fn workflow_macro_builds_graph_with_implicit_and_pinned_deps() {
        let workflow = workflow! {
            name: "Graph Macro",
            description: "Composite graph declared in workflow!.",
            input "value": "json" {
                description: "Payload.",
                required: true,
            }
            output "result": "text" {
                description: "Result.",
            }
            node prompt: "lightflow.text_prompt",
            node result: "lightflow.text_result" @ "0.1.0",
            edge prompt.prompt -> result.text,
        }
        .build();

        assert_eq!(workflow.name, "Graph Macro");
        assert_eq!(
            workflow.description.as_deref(),
            Some("Composite graph declared in workflow!.")
        );
        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.nodes[0].id, "prompt");
        assert_eq!(workflow.nodes[0].workflow_id, "lightflow.text_prompt");
        assert_eq!(workflow.nodes[1].id, "result");
        assert_eq!(workflow.edges.len(), 1);
        assert_eq!(workflow.edges[0].from.node, "prompt");
        assert_eq!(workflow.edges[0].from.port, "prompt");
        assert_eq!(workflow.edges[0].to.node, "result");
        assert_eq!(workflow.edges[0].to.port, "text");
        assert_eq!(
            workflow
                .dependencies
                .iter()
                .map(|dependency| (
                    dependency.workflow_id.as_str(),
                    dependency.version.as_deref()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("lightflow.text_prompt", None),
                ("lightflow.text_result", Some("0.1.0")),
            ]
        );
    }

    #[test]
    fn node_merges_with_explicit_depends_on_version() {
        let workflow = workflow_with_identity("lightflow.example", "0.1.0")
            .node("child", "lightflow.child")
            .depends_on("lightflow.child", "0.2.0")
            .build();

        assert_eq!(workflow.dependencies.len(), 1);
        assert_eq!(workflow.dependencies[0].workflow_id, "lightflow.child");
        assert_eq!(workflow.dependencies[0].version.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn wire_parses_dotted_endpoints() {
        let workflow = workflow_with_identity("lightflow.example", "0.1.0")
            .node("a", "lightflow.a")
            .node("b", "lightflow.b")
            .wire("a.out", "b.in")
            .build();

        assert_eq!(workflow.edges[0].from.port, "out");
        assert_eq!(workflow.edges[0].to.port, "in");
    }

    #[test]
    #[should_panic(expected = "invalid endpoint")]
    fn wire_rejects_endpoint_without_dot() {
        let _ = workflow_with_identity("lightflow.example", "0.1.0").wire("a", "b.in");
    }
}
