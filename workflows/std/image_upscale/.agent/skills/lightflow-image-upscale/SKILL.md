---
name: LightFlow Image Upscale
description: Use this skill when working with the lightflow.image.upscale workflow that enlarges PNG image artifacts.
version: 0.1.0
---

# LightFlow Image Upscale

Use `lightflow.image.upscale` to enlarge a PNG image by an integer scale factor.

## Workflow

- Workflow id: `lightflow.image.upscale`
- Runtime: `lightflow.image.upscale`.
- Input `image_path`: required source PNG path; artifact kind `image`; widget `image`.
- Input `scale`: optional integer; default `2`.
- Outputs `image` and `image_path`; artifact kind `image`.

## Usage

```bash
lfw run lightflow.image.upscale -i image_path=./input.png -i scale=2
```
