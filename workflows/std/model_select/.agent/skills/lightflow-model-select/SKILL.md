---
name: LightFlow Model Select
description: Use this skill when working with the lightflow.model.select workflow that chooses a model variant from JSON metadata.
version: 0.1.0
---

# LightFlow Model Select

Use `lightflow.model.select` to choose a model variant by id or format.

## Workflow

- Workflow id: `lightflow.model.select`
- Runtime: `lightflow.model.select`.
- Input `variants`: required JSON array.
- Input `preferred`: optional variant id or format.
- Outputs `model`, `variant_id`, and `requirement_id`.

## Usage

```bash
lfw run lightflow.model.select -i requirement_id=image_model -i preferred=gguf -i variants='[{"id":"q4","format":"gguf"}]'
```
