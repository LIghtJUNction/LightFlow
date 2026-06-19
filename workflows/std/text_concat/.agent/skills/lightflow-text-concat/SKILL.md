---
name: LightFlow Text Concat
description: Use this skill when working with the lightflow.text.concat workflow that joins text values for prompts or labels.
version: 0.1.0
---

# LightFlow Text Concat

Use `lightflow.text.concat` to join two text inputs or an array of values.

## Workflow

- Workflow id: `lightflow.text.concat`
- Runtime: `lightflow.text.concat`.
- Inputs `a` and `b`: optional text values; widget `textarea`.
- Input `items`: optional JSON array; widget `json`.
- Input `separator`: optional text separator; default `""`.
- Output `text`: concatenated text.

## Usage

```bash
lfw run lightflow.text.concat -i a=hello -i b=world -i separator=' '
```
