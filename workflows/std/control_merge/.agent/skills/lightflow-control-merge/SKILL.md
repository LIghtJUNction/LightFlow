---
name: LightFlow Control Merge
description: Use this skill when working with the lightflow.control.merge workflow that merges two JSON values.
version: 0.1.0
---

# LightFlow Control Merge

Use `lightflow.control.merge` to combine two JSON values.

## Workflow

- Workflow id: `lightflow.control.merge`
- Runtime: `lightflow.control.merge`.
- Inputs `a` and `b`: optional JSON values.
- Input `mode`: `first_non_null`, `object`, or `array`; default `first_non_null`.
- Outputs `value` and `selected`.

## Usage

```bash
lfw run lightflow.control.merge -i a='{"prompt":"cat"}' -i b='{"seed":1}' -i mode=object
```
