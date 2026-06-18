# Workflows

Each top-level directory is one category. Workflow crates live one level below
that category, for example `std/text_plan/src/lib.rs`. Reusable workflows define
`src/lib.rs` and do not define `src/main.rs`. Leaf workflows declare ports and
no nodes. Composite workflows use `.node(..., workflow_id)` to nest other
workflows.

`lightflow.std` is a normal workflow crate in this repository, not a backend
built-in. It is reserved for minimal, abstract, reusable building blocks and
must not contain agent behavior, provider integrations, or business templates.

`lightflow.text_plan` depends on and nests `lightflow.std` to verify that the
standard workflow path is exercised by a real local workflow.
