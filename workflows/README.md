# Workflows

The core repository no longer keeps standard workflow crates in this directory.
Reusable workflow families live as sibling project repositories under
`projects/`, for example `projects/lightflow-std/workflows/std/text_plan`.
Reusable workflows define `src/lib.rs` and do not define `src/main.rs`. Leaf
workflows declare ports and no nodes. Composite workflows use
`.node(..., workflow_id)` to nest other workflows.

Every standard node is a separate workflow crate in the `lightflow-std`
project; there is no aggregate `std/std` workflow crate.

`lightflow.text_plan` composes `lightflow.text_prompt` and
`lightflow.text_result` to exercise the standard workflow project.
