use super::{ApiResult, ApiService};

mod artifacts;
mod checks;
mod commands;
mod config;
mod loop_checks;
mod project_checks;
mod types;
mod utils;

pub use types::{
    CheckProfile, ReleaseCheck, ReleaseCheckKind, ReleaseCheckOptions, ReleaseCheckReport,
    ReleaseCheckStatus,
};

impl ApiService {
    pub fn release_check(&self, options: &ReleaseCheckOptions) -> ApiResult<ReleaseCheckReport> {
        release_check(self, options)
    }
}

fn release_check(
    service: &ApiService,
    options: &ReleaseCheckOptions,
) -> ApiResult<ReleaseCheckReport> {
    let root = service.repo_root();
    let mut checks = Vec::new();
    if options.profile == CheckProfile::Release {
        for (id, path) in artifacts::release_artifacts() {
            checks.push(artifacts::artifact_check(root, id, path));
        }
        for (id, path, needle, description) in artifacts::release_document_checks() {
            checks.push(artifacts::document_check(
                root,
                id,
                path,
                needle,
                description,
            )?);
        }
    }
    checks.push(
        loop_checks::local_workflow_loop_review_check(service, options.project.as_deref())
            .unwrap_or_else(|error| {
                checks::review_error_check(
                    "release.review.local_workflow_loop",
                    "local workflow loop could not be inspected",
                    error,
                    None,
                )
            }),
    );
    checks.push(
        loop_checks::selected_workflow_loop_review_check(service, &options.workflow_id)
            .unwrap_or_else(|error| {
                checks::review_error_check(
                    "release.review.selected_workflow_loop",
                    "selected workflow loop could not be inspected",
                    error,
                    None,
                )
            }),
    );
    checks.push(
        project_checks::source_change_review_check(service).unwrap_or_else(|error| {
            checks::review_error_check(
                "release.review.workflow_change_skills",
                "source-change safety could not be inspected",
                error,
                None,
            )
        }),
    );
    checks.push(project_checks::project_workspace_review_check(
        service,
        options.project.as_deref(),
    ));
    checks.push(
        project_checks::workflow_publish_review_check(service, options.project.as_deref())
            .unwrap_or_else(|error| {
                checks::review_error_check(
                    "release.review.workflow_publish_ready",
                    "workflow publish readiness could not be inspected",
                    error,
                    None,
                )
            }),
    );

    let mut apply_blocked = options.apply
        && checks
            .iter()
            .any(|check| check.status == ReleaseCheckStatus::Failed);

    for (id, command) in
        commands::release_commands(&options.workflow_id, options.project.as_deref())
    {
        if apply_blocked {
            checks.push(commands::command_skipped_check(id, command));
            continue;
        }
        let check = commands::command_check(root, id, command, options.apply)?;
        if options.apply && check.status == ReleaseCheckStatus::Failed {
            apply_blocked = true;
        }
        checks.push(check);
    }

    let valid = !checks
        .iter()
        .any(|check| check.status == ReleaseCheckStatus::Failed);
    let issues = checks::release_issues(&checks);
    let warnings = checks::release_warnings(&checks);
    let passed = checks::release_check_count(&checks, ReleaseCheckStatus::Passed);
    let warning_count = checks::release_check_count(&checks, ReleaseCheckStatus::Warning);
    let failed = checks::release_check_count(&checks, ReleaseCheckStatus::Failed);
    let planned = checks::release_check_count(&checks, ReleaseCheckStatus::Planned);
    let skipped = checks::release_check_count(&checks, ReleaseCheckStatus::Skipped);

    let known_project_workspaces = config::known_project_workspaces(service);
    let known_project_aliases = config::known_project_aliases(service);
    let project_filter_matched =
        config::project_filter_matched(service, options.project.as_deref());
    let matched_project_workspace =
        config::matched_project_workspace(service, options.project.as_deref());
    let project_config = config::project_config_report(service);

    Ok(ReleaseCheckReport {
        profile: options.profile,
        dry_run: !options.apply,
        valid,
        project_root: root.to_path_buf(),
        workflow_id: options.workflow_id.clone(),
        project_config_path: project_config.path,
        project_config_present: project_config.present,
        project_config_valid: project_config.valid,
        project_config_error: project_config.error,
        project_config_template_command: project_config.template_command,
        project_config_write_command: project_config.write_command,
        project_submodule_update_command: project_config.submodule_update_command,
        default_workflow_sources: project_config.default_workflow_sources,
        known_optional_workspace_names: project_config.known_optional_workspace_names,
        project: options.project.clone(),
        project_filter_matched,
        matched_project_workspace,
        known_project_workspaces,
        known_project_aliases,
        issues,
        warnings,
        passed,
        warning_count,
        failed,
        planned,
        skipped,
        checks,
    })
}
