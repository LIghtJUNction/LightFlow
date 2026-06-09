# Kernel Policy

LightFlow is not a Linux kernel subsystem candidate.

The local Linux execution path is:

```text
LightFlow service / CLI / MCP / HTTP
  -> CortexFS userspace daemon
  -> Linux FUSE interface
  -> existing kernel VFS, permission, cgroup, namespace, audit, and process primitives
```

The `/ctx` tree is a userspace ABI owned by CortexFS and LightFlow. It is not a proposed Linux kernel ABI.

## Hard Boundary

Keep these concepts in userspace:

- workflow graphs and pipeline run records
- AI provider routing and model aliases
- MCP, tool, hook, and structured job protocols
- HTTP, OpenAPI, WebTransport, FlatBuffers stream metadata, and JSON request bodies
- policy decisions, approvals, secret references, and provider credentials
- audit correlation across LightFlow and CortexFS

None of those belong in `fs/`, `drivers/`, `kernel/`, or any other Linux kernel tree directory.

## What Could Be Upstreamed

Only upstream a kernel change when LightFlow or CortexFS exposes a generic Linux limitation that affects userspace filesystems or process isolation broadly.

Examples of potentially valid kernel-facing work:

- a FUSE capability that avoids a measurable userspace filesystem bottleneck
- an fsnotify/fanotify improvement needed by many file-backed runtimes
- a cgroup, namespace, or LSM primitive that improves generic isolation
- a tracepoint or selftest for an existing kernel interface

The unit of upstreaming is the primitive, not the product.

## Patch Bar

Before proposing any kernel patch, require:

- a minimal reproducer independent of LightFlow branding
- benchmark numbers showing the current kernel limitation and the improvement
- selftests or fstests where applicable
- documentation for any new ABI or behavior
- a small patch series sent to the relevant maintainer list
- no provider, model, workflow, HTTP, MCP, OpenAPI, or JSON product protocol in the patch

If the patch cannot be explained without mentioning AI workflow products, it is probably not a kernel patch.

## Project Rule

LightFlow exposes this policy with:

```bash
lightflow ctx abi
```

and over HTTP:

```text
GET /ctx/abi
```

Clients and agents should treat `kernel_tree: false` as project policy.
