use super::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    RuntimeRequirement, WorkflowCondition, WorkflowDependencyRequirement, WorkflowEdge,
    WorkflowEndpoint, WorkflowNode, WorkflowNodeKind, WorkflowPosition, WorkflowSpec,
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
        self.spec.dependencies.push(WorkflowDependencyRequirement {
            workflow_id: workflow_id.into(),
            version: Some(version.into()),
            install: None,
        });
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
        self.spec.dependencies.push(WorkflowDependencyRequirement {
            workflow_id: workflow_id.into(),
            version: Some(version.clone()),
            install: Some(CargoDependency {
                crate_name: crate_name.into(),
                version: Some(version),
                source: None,
                package: None,
            }),
        });
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
        self.spec.dependencies.push(WorkflowDependencyRequirement {
            workflow_id: workflow_id.into(),
            version: Some(version.clone()),
            install: Some(CargoDependency {
                crate_name: crate_name.into(),
                version: Some(version),
                source: Some(CargoDependencySource::Path(path.into())),
                package: None,
            }),
        });
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
        self.spec.dependencies.push(WorkflowDependencyRequirement {
            workflow_id: workflow_id.into(),
            version: Some(version.clone()),
            install: Some(CargoDependency {
                crate_name: crate_name.into(),
                version: Some(version),
                source: Some(CargoDependencySource::Git(git.into())),
                package: Some(package).filter(|package| !package.is_empty()),
            }),
        });
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

    #[must_use]
    pub fn node(mut self, id: impl Into<String>, workflow_id: impl Into<String>) -> Self {
        self.spec.nodes.push(WorkflowNode {
            id: id.into(),
            kind: WorkflowNodeKind::Workflow,
            workflow_id: workflow_id.into(),
            condition: None,
            then_workflow_id: None,
            else_workflow_id: None,
            title: None,
            disabled: false,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        });
        self
    }

    #[must_use]
    pub fn disabled_node(mut self, id: impl Into<String>, workflow_id: impl Into<String>) -> Self {
        self.spec.nodes.push(WorkflowNode {
            id: id.into(),
            kind: WorkflowNodeKind::Workflow,
            workflow_id: workflow_id.into(),
            condition: None,
            then_workflow_id: None,
            else_workflow_id: None,
            title: None,
            disabled: true,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        });
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
        self.spec.nodes.push(WorkflowNode {
            id: id.into(),
            kind: WorkflowNodeKind::If,
            workflow_id: String::new(),
            condition: Some(WorkflowCondition::InputEquals {
                input: input.into(),
                value: serde_json::Value::Bool(expected),
            }),
            then_workflow_id: Some(then_workflow_id.into()),
            else_workflow_id: Some(else_workflow_id.into()),
            title: None,
            disabled: false,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        });
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

    #[must_use]
    pub fn build(self) -> WorkflowSpec {
        self.spec
    }
}

impl From<WorkflowBuilder> for WorkflowSpec {
    fn from(builder: WorkflowBuilder) -> Self {
        builder.spec
    }
}
