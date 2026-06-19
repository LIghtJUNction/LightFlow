---
name: LightFlow Text To Image
description: Use this skill when working with the lightflow.text_to_image preview workflow, its image inputs, outputs, model sync, or pipeline composition.
version: 0.1.0
---

# LightFlow Text To Image

Use `lightflow.text_to_image` for the built-in preview image runtime. This workflow writes a deterministic preview PNG and is not a real diffusion model executor.

## Workflow

- Workflow id: `lightflow.text_to_image`
- Input `prompt`: required positive prompt; widget `prompt`.
- Input `negative`: optional negative prompt; default `""`; widget `textarea`.
- Input `width`: optional integer; default `512`; range `64..2048`; step `8`; widget `number`.
- Input `height`: optional integer; default `512`; range `64..2048`; step `8`; widget `number`.
- Input `seed`: optional integer seed; widget `seed`.
- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.
- Input `model`: optional model variant id bound to `image_model`; choices `sdxl-gguf-q4` or `sdxl-safetensors`; widget `model_select`.
- Outputs: `image` artifact metadata and `image_path`; artifact kind `image`; bound to `image_model`.
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
