# Workflows

The core repository no longer keeps standard workflow crates in this directory.
Reusable workflow families live as sibling project repositories under
`projects/`, for example `projects/lightflow-std/workflows/text_plan`.
Reusable workflows define `src/lib.rs` and do not define `src/main.rs`. Leaf
workflows declare ports and no nodes. Composite workflows use
`.node(..., workflow_id)` to nest other workflows.

Every standard node is an independently publishable and consumable workflow
crate in the `lightflow-std` project.

`lightflow.text_plan` composes `lightflow.text_prompt` and
`lightflow.text_result` to exercise the standard workflow project.
