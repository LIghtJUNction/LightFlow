---
name: LightFlow Image Resize
description: Use this skill when working with the lightflow.image.resize workflow that resizes PNG image artifacts.
version: 0.1.0
---

# LightFlow Image Resize

Use `lightflow.image.resize` to resize a PNG image with nearest-neighbor sampling.

## Workflow

- Workflow id: `lightflow.image.resize`
- Runtime: `lightflow.image.resize`.
- Input `image_path`: required source PNG path; artifact kind `image`; widget `image`.
- Inputs `width` and `height`: required output dimensions; widget `number`.
- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.
- Outputs `image` and `image_path`; artifact kind `image`.

## Usage

```bash
lfw run lightflow.image.resize -i image_path=./input.png -i width=512 -i height=512
```
