---
name: LightFlow Image Edit
description: Use this skill when working with the lightflow.image.edit workflow that creates deterministic preview image edits.
version: 0.1.0
---

# LightFlow Image Edit

Use `lightflow.image.edit` for image-to-image preview workflows. The builtin runtime creates deterministic preview pixels from the source image and prompt; a model-backed executor can replace it later without changing the workflow interface.

## Workflow

- Workflow id: `lightflow.image.edit`
- Runtime: `lightflow.image.edit` with engine `builtin.preview.edit.v1`.
- Inputs: `image_path`, `prompt`, optional `negative`, optional `seed`, optional `output_path`.
- Outputs: `image`, `image_path`, `prompt`.

## Usage

```bash
lfw run lightflow.image.edit -i image_path=input.png -i prompt='make the lighting warmer'
```
