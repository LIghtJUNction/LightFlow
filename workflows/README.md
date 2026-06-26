# Workflows

The core repository no longer keeps standard workflow crates in this directory.
Reusable workflow families live as sibling project repositories under
`projects/`, for example `projects/lightflow-std/workflows/std/text_plan`.
Reusable workflows define `src/lib.rs` and do not define `src/main.rs`. Leaf
workflows declare ports and no nodes. Composite workflows use
`.node(..., workflow_id)` to nest other workflows.

`lightflow.std` is a normal workflow crate in the `lightflow-std` project, not a
backend built-in. It is reserved for minimal, abstract, reusable building blocks
and must not contain agent behavior, provider integrations, or business
templates.

`lightflow.text_plan` depends on and nests `lightflow.std` to verify that the
standard workflow project is exercised by a real local workflow.
