---
name: lightflow-a
description: Use the lightflow.a ABC example workflow, install its B/C dependencies, and run its control-flow branch.
version: 0.1.0
---

# lightflow.a

`lightflow.a` is a composite workflow that depends on `lightflow.b` and
`lightflow.c`.

Install the sibling workflow projects before running:

```bash
lfw add lightflow-b --path ../lightflow-b/workflows/abc/b --editable
lfw add lightflow-c --path ../lightflow-c/workflows/abc/c
```

Run the B branch:

```bash
lfw run lightflow.a -i use_b=true -i value=hello
```

Run the C branch:

```bash
lfw run lightflow.a -i use_b=false -i value=hello
```
