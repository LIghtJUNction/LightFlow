---
name: LightFlow Image Crop
description: Use this skill when working with the lightflow.image.crop workflow that crops PNG image artifacts.
version: 0.1.0
---

# LightFlow Image Crop

Use `lightflow.image.crop` to crop a rectangular region from a PNG image.

## Workflow

- Workflow id: `lightflow.image.crop`
- Runtime: `lightflow.image.crop`.
- Input `image_path`: required source PNG path; artifact kind `image`; widget `image`.
- Inputs `x`, `y`, `width`, and `height`: crop rectangle; widget `number`.
- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.
- Outputs `image` and `image_path`; artifact kind `image`.

## Usage

```bash
lfw run lightflow.image.crop -i image_path=./input.png -i x=0 -i y=0 -i width=512 -i height=512
```
