---
name: LightFlow Text To Image
description: Use this skill when working with the lightflow.text_to_image preview workflow, its image inputs, outputs, model sync, or pipeline composition.
version: 0.1.0
---

# LightFlow Text To Image

Use `lightflow.text_to_image` for the built-in preview image runtime. This workflow writes a deterministic preview PNG and is not a real diffusion model executor.

## Workflow

- Workflow id: `lightflow.text_to_image`
- Inputs: `prompt`, `negative`, `width`, `height`, `seed`, `output_path`, `model`.
- Outputs: `image`, `image_path`.
- Runtime: `lightflow.image.generate` with engine `builtin.preview.v1`.
- Default output directory: XDG Pictures under `lightflow/`.

## Usage

```bash
lfw run lightflow.text_to_image \
  -i prompt='"a quiet lake"' \
  -i width=512 \
  -i height=512 \
  -i output_path='"out/lake.png"'
```
