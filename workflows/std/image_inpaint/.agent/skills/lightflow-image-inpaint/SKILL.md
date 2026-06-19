---
name: LightFlow Image Inpaint
description: Use this skill when working with the lightflow.image.inpaint workflow that creates deterministic preview inpaint images.
version: 0.1.0
---

# LightFlow Image Inpaint

Use `lightflow.image.inpaint` for masked image editing preview workflows. The builtin runtime edits source pixels according to mask luminance; a model-backed executor can replace it later without changing the workflow interface.

## Workflow

- Workflow id: `lightflow.image.inpaint`
- Runtime: `lightflow.image.inpaint` with engine `builtin.preview.inpaint.v1`.
- Inputs: `image_path`, `mask_path`, `prompt`, optional `negative`, optional `seed`, optional `output_path`.
- Outputs: `image`, `image_path`, `prompt`.

## Usage

```bash
lfw run lightflow.image.inpaint -i image_path=input.png -i mask_path=mask.png -i prompt='repair the damaged area'
```
