use super::super::{ApiResult, ApiService, LocalLoopCheck};

pub(in crate::api::loop_check) fn push_publish_check(
    service: &ApiService,
    checks: &mut Vec<LocalLoopCheck>,
) -> ApiResult<()> {
    match service.workflow_publish_checks() {
        Ok(catalog) if catalog.total == 0 => {
            checks.push(LocalLoopCheck::warning(
                "loop.publish.workflow_crates",
                "no workflow crates found for lfw publish --workflows",
            ));
        }
        Ok(catalog) if catalog.publishable => {
            checks.push(
                LocalLoopCheck::passed(
                    "loop.publish.workflow_crates",
                    "workflow crates are present for lfw publish --workflows",
                )
                .count(catalog.total),
            );
            checks.push(
                LocalLoopCheck::passed(
                    "loop.publish.readiness",
                    "all workflow crates pass publish preflight checks",
                )
                .count(catalog.checks.len()),
            );
        }
        Ok(catalog) => {
            checks.push(
                LocalLoopCheck::passed(
                    "loop.publish.workflow_crates",
                    "workflow crates are present for lfw publish --workflows",
                )
                .count(catalog.total),
            );
            let blocked = catalog
                .checks
                .iter()
                .filter(|check| !check.publishable)
                .count();
            checks.push(
                LocalLoopCheck::warning(
                    "loop.publish.readiness",
                    format!(
                        "{blocked} of {} workflow crates are not publishable yet; inspect /publish or lfw publish --workflows",
                        catalog.checks.len()
                    ),
                )
                .count(catalog.issues.len()),
            );
        }
        Err(error) => checks.push(LocalLoopCheck::failed(
            "loop.publish.readiness",
            format!("workflow publish readiness could not be inspected: {error}"),
        )),
    }
    Ok(())
}
