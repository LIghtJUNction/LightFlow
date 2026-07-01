# Development Guidelines

When making code changes in LightFlow, read these three documents first:

- `README.md`
- `docs/architecture.md`
- `docs/data-layout.md`

## Source Shape

To keep workflow refactors predictable and reviewable:

- Avoid splitting source by numbering files such as `001_*`, `002_*`, `123_*`, or any three-digit-prefix filename.
- Prefer semantic module splits (for example `workflow.rs`, `execution.rs`, `execution/text.rs`) when a file approaches or exceeds `500` lines.
- Keep first-party Rust modules, workflow crates, macro crates, and integration tests under meaningful names for discoverability.
- If a file is generated and intentionally exceeds these conventions, document the exception in the file header and this repository's source-shape checks.

## Refactors

- Preserve public API shape: function signatures, serialization formats, and externally visible error semantics should remain behaviorally equivalent unless a breaking-change plan is explicitly approved.
- Keep data-model fields and API contracts stable during reshuffles.
- Limit changes to file organization and boundaries unless product behavior is intentionally in scope.

## Verification

After code changes that touch Rust behavior, run at least:

- `cargo fmt --check`
- `cargo test --lib`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

Also run the source-shape check:

- `scripts/check-source-shape.sh`

## Test Repository Hygiene

For tests that create temporary git repositories and commits, use deterministic git helpers (for example the helper that sets fixed `user.name`, `user.email`, and `commit.gpgsign=false`) so commit hashes and test output are reproducible.
