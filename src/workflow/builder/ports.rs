use super::WorkflowBuilder;
use crate::workflow::PortSpec;

impl WorkflowBuilder {
    #[must_use]
    pub fn input(mut self, name: impl Into<String>, ty: impl Into<String>) -> Self {
        self.spec.inputs.push(PortSpec::new(name, ty));
        self
    }

    #[must_use]
    pub fn output(mut self, name: impl Into<String>, ty: impl Into<String>) -> Self {
        self.spec.outputs.push(PortSpec::new(name, ty));
        self
    }

    #[must_use]
    pub fn input_description(
        mut self,
        name: impl AsRef<str>,
        description: impl Into<String>,
    ) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.description = Some(description.into());
        }
        self
    }

    #[must_use]
    pub fn output_description(
        mut self,
        name: impl AsRef<str>,
        description: impl Into<String>,
    ) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.outputs, name.as_ref()) {
            port.description = Some(description.into());
        }
        self
    }

    #[must_use]
    pub fn input_required(mut self, name: impl AsRef<str>, required: bool) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.required = Some(required);
        }
        self
    }

    #[must_use]
    pub fn input_default_json(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.default =
                Some(serde_json::from_str(value.as_ref()).expect("default must be valid JSON"));
        }
        self
    }

    #[must_use]
    pub fn input_default(mut self, name: impl AsRef<str>, value: serde_json::Value) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.default = Some(value);
        }
        self
    }

    #[must_use]
    pub fn input_range(mut self, name: impl AsRef<str>, min: f64, max: f64, step: f64) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.min = Some(min);
            port.max = Some(max);
            port.step = Some(step);
        }
        self
    }

    #[must_use]
    pub fn input_enum_json(mut self, name: impl AsRef<str>, values: impl AsRef<str>) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.enum_values = serde_json::from_str(values.as_ref())
                .expect("enum values must be a valid JSON array");
        }
        self
    }

    #[must_use]
    pub fn input_choices(mut self, name: impl AsRef<str>, values: serde_json::Value) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.enum_values = values
                .as_array()
                .cloned()
                .expect("choices must be a JSON array");
        }
        self
    }

    #[must_use]
    pub fn input_widget(mut self, name: impl AsRef<str>, widget: impl Into<String>) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.widget = Some(widget.into());
        }
        self
    }

    #[must_use]
    pub fn input_artifact_kind(mut self, name: impl AsRef<str>, kind: impl Into<String>) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.artifact_kind = Some(kind.into());
        }
        self
    }

    #[must_use]
    pub fn output_artifact_kind(mut self, name: impl AsRef<str>, kind: impl Into<String>) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.outputs, name.as_ref()) {
            port.artifact_kind = Some(kind.into());
        }
        self
    }

    #[must_use]
    pub fn input_model_requirement(
        mut self,
        name: impl AsRef<str>,
        requirement_id: impl Into<String>,
    ) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.inputs, name.as_ref()) {
            port.model_requirement = Some(requirement_id.into());
        }
        self
    }

    #[must_use]
    pub fn output_model_requirement(
        mut self,
        name: impl AsRef<str>,
        requirement_id: impl Into<String>,
    ) -> Self {
        if let Some(port) = find_port_mut(&mut self.spec.outputs, name.as_ref()) {
            port.model_requirement = Some(requirement_id.into());
        }
        self
    }
}

fn find_port_mut<'a>(ports: &'a mut [PortSpec], name: &str) -> Option<&'a mut PortSpec> {
    ports.iter_mut().find(|port| port.name == name)
}
