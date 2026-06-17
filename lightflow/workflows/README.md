# Workflows

Each `.rs` file defines one workflow with embedded metadata and graph
structure. Leaf workflows declare ports and no nodes. Composite workflows use
`.node(..., workflow_id)` to nest other workflows.
