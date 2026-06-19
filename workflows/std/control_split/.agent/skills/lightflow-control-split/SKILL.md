---
name: LightFlow Control Split
description: Use this skill when working with the lightflow.control.split workflow that splits JSON arrays or objects.
version: 0.1.0
---

# LightFlow Control Split

Use `lightflow.control.split` to turn a JSON array or object into `first`, `rest`, and `items` outputs.

## Workflow

- Workflow id: `lightflow.control.split`
- Runtime: `lightflow.control.split`.
- Input `value`: required JSON value.
- Outputs `first`, `rest`, and `items`.

## Usage

```bash
lfw run lightflow.control.split -i value='["prompt","negative","seed"]'
```
