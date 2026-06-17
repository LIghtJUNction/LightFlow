# lightflow.std

`lightflow.std` is the standard workflow crate for LightFlow. It is not a
backend built-in; users install or depend on it like any other workflow crate.

## Scope

Allowed:

- identity / passthrough
- no-op control points
- structural merge / split helpers
- basic type adapters when they are domain-neutral

Not allowed:

- agent behavior
- model provider integrations
- model download logic
- business templates
- product-specific defaults
- workflow execution policy

Reusable standard workflows define `src/lib.rs` and do not define `src/main.rs`.
Adding `src/main.rs` would mark a workflow crate as an executable entrypoint,
which is outside the scope of this standard library crate.
