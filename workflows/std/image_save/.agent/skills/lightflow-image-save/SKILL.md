---
name: LightFlow Image Save
description: Use this skill when working with the lightflow.image.save workflow that copies PNG images to chosen output paths.
version: 0.1.0
---

# LightFlow Image Save

Use `lightflow.image.save` to copy a PNG image to a chosen output path.

## Workflow

- Workflow id: `lightflow.image.save`
- Runtime: `lightflow.image.save`.
- Input `image_path`: required source PNG path; artifact kind `image`; widget `image`.
- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.
- Outputs `image` and `image_path`; artifact kind `image`.

## Usage

```bash
lfw run lightflow.image.save -i image_path=./input.png -i output_path=./out.png
```
