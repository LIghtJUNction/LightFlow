---
name: LightFlow LLM Structured Output
description: Use this skill when working with the lightflow.llm.structured_output workflow that turns LLM text into JSON.
version: 0.1.0
---

# LightFlow LLM Structured Output

Use `lightflow.llm.structured_output` to parse JSON text or wrap plain text as `{ "text": ... }`.

## Workflow

- Workflow id: `lightflow.llm.structured_output`
- Runtime: `lightflow.llm.structured_output`.
- Input `text`: required text or JSON string.
- Input `schema`: optional JSON schema metadata.
- Outputs `object` and `json`.

## Usage

```bash
lfw run lightflow.llm.structured_output -i text='{"intent":"search"}'
```
