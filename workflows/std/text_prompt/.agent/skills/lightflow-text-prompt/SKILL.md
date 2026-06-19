---
name: LightFlow Text Prompt
description: Use this skill when working with the lightflow.text_prompt workflow that turns structured input into prompt text.
version: 0.1.0
---

# LightFlow Text Prompt

Use `lightflow.text_prompt` to normalize structured input into a text prompt.

## Workflow

- Workflow id: `lightflow.text_prompt`
- Input `value`: required JSON source value; widget `json`.
- Output `prompt`: text prompt generated from the source value.

## Usage

```bash
lfw run lightflow.text_prompt -i value='"repair the masked area naturally"'
```
