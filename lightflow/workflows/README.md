# Workflows

Each directory is one workflow crate. Reusable workflows define `src/lib.rs`
and do not define `src/main.rs`. Leaf workflows declare ports and no nodes.
Composite workflows use `.node(..., workflow_id)` to nest other workflows.
