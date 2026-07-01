use super::{
    ApiService, PatchCatalog, PatchValidation, RegisteredPatch, RemovedPatch, SavedPatch, patches,
};

impl ApiService {
    /// List project-local reusable workflow patches.
    pub fn list_patches(&self) -> super::ApiResult<PatchCatalog> {
        patches::list_patches(&self.repo_root)
    }

    /// Read one project-local reusable workflow patch.
    pub fn get_patch(&self, name: &str) -> super::ApiResult<RegisteredPatch> {
        patches::get_patch(&self.repo_root, name)
    }

    /// Save one project-local reusable workflow patch.
    pub fn save_patch(
        &self,
        name: &str,
        patch: &crate::workflow::WorkflowPatch,
    ) -> super::ApiResult<SavedPatch> {
        patches::save_patch(&self.repo_root, name, patch)
    }

    /// Remove one project-local reusable workflow patch.
    pub fn remove_patch(&self, name: &str) -> super::ApiResult<RemovedPatch> {
        patches::remove_patch(&self.repo_root, name)
    }

    /// Validate a serializable workflow patch payload.
    pub fn validate_patch(&self, patch: crate::workflow::WorkflowPatch) -> PatchValidation {
        match self.workflow_specs() {
            Ok(workflows) => patches::validate_patch(patch, Some(&workflows)),
            Err(error) => PatchValidation {
                valid: false,
                issues: vec![format!("workflow catalog could not be inspected: {error}")],
                patch,
            },
        }
    }

    /// Validate a serializable workflow patch against one selected workflow.
    pub fn validate_patch_for_workflow(
        &self,
        workflow_id: &str,
        patch: crate::workflow::WorkflowPatch,
    ) -> PatchValidation {
        match self.workflow_specs() {
            Ok(workflows) => match workflows.get(workflow_id) {
                Some(workflow) => {
                    patches::validate_patch_for_workflow(&patch, workflow, &workflows)
                }
                None => PatchValidation {
                    valid: false,
                    issues: vec![format!("workflow {workflow_id} not found")],
                    patch,
                },
            },
            Err(error) => PatchValidation {
                valid: false,
                issues: vec![format!("workflow catalog could not be inspected: {error}")],
                patch,
            },
        }
    }
}
