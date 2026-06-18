# ABC Workflow Projects

This directory contains three independent LightFlow workflow projects used as a
small dependency-resolution fixture:

- `lightflow-a` defines `lightflow.a`.
- `lightflow-b` defines `lightflow.b`.
- `lightflow-c` defines `lightflow.c`.

`lightflow.a` declares install hints for `lightflow.b` and `lightflow.c`, then
uses an `if_node` to choose one branch at runtime.

```bash
cd examples/abc/lightflow-a
lfw add lightflow-b --path ../lightflow-b/workflows/abc/b --editable
lfw add lightflow-c --path ../lightflow-c/workflows/abc/c
lfw deps lightflow.a
lfw run lightflow.a -i use_b=true -i value=hello
```
