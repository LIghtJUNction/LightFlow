---
name: LightFlow LLM Classify
description: Use this skill when working with the lightflow.llm.classify workflow that classifies text into labels offline.
version: 0.1.0
---

# LightFlow LLM Classify

Use `lightflow.llm.classify` to choose a label from a JSON label array. The builtin implementation is deterministic and offline.

## Workflow

- Workflow id: `lightflow.llm.classify`
- Runtime: `lightflow.llm.classify`.
- Input `text`: required text.
- Input `labels`: required JSON array of labels.
- Outputs `label` and `confidence`.

## Usage

```bash
lfw run lightflow.llm.classify -i text='urgent billing issue' -i labels='["billing","support"]'
```
