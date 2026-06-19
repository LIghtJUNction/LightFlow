---
name: LightFlow LLM Generate
description: Use this skill when working with the lightflow.llm.generate workflow that provides offline mock LLM generation.
version: 0.1.0
---

# LightFlow LLM Generate

Use `lightflow.llm.generate` for deterministic offline mock text generation. Provider-backed RIG workflows can still declare `lightflow.llm.generate` without the builtin mock engine.

## Workflow

- Workflow id: `lightflow.llm.generate`
- Runtime: `lightflow.llm.generate`, engine `builtin.llm.mock.v1`.
- Input `prompt`: required prompt text.
- Input `model`: optional mock model name.
- Outputs `text` and `response`.

## Usage

```bash
lfw run lightflow.llm.generate -i prompt='hello' -i model=mock
```
