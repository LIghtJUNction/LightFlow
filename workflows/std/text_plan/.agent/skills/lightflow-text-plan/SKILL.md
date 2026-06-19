---
name: LightFlow Text Plan
description: Use this skill when working with the lightflow.text_plan composite example workflow.
version: 0.1.0
---

# LightFlow Text Plan

Use `lightflow.text_plan` as a small example of composing workflow nodes and edges.

## Workflow

- Workflow id: `lightflow.text_plan`
- Input `value`: required JSON value passed into the plan; widget `json`.
- Output `result`: normalized text result.
- Dependencies: `lightflow.std`, `lightflow.text_prompt`, `lightflow.text_result`.

## Usage

```bash
lfw run lightflow.text_plan -i value='"hello"'
```
