# Workflows

Each directory is one workflow crate. Reusable workflows have `src/lib.rs` and
no `src/main.rs`, so they are imported by other workflows instead of executed as
entrypoints.
