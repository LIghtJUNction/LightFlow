---
name: LightFlow Control If
description: Use this skill when working with the lightflow.control.if workflow that selects between two JSON values.
version: 0.1.0
---

# LightFlow Control If

Use `lightflow.control.if` to choose between `then_value` and `else_value`.

## Workflow

- Workflow id: `lightflow.control.if`
- Runtime: `lightflow.control.if`.
- Input `condition`: required boolean; widget `toggle`.
- Inputs `then_value` and `else_value`: optional JSON values.
- Outputs `value` and `selected`.

## Usage

```bash
lfw run lightflow.control.if -i condition=true -i then_value='"yes"' -i else_value='"no"'
```
