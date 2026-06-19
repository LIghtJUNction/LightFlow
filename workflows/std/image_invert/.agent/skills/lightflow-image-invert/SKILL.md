---
name: LightFlow Image Invert
description: Use this skill when working with the lightflow.image.invert standard workflow, inverting PNG outputs, or composing image inversion after another LightFlow image workflow.
version: 0.1.0
---

# LightFlow Image Invert

Use this skill to invert the colors of a PNG image generated or referenced by a LightFlow workflow.

## Workflow

- Workflow id: `lightflow.image.invert`
- Input `image_path`: required source PNG path; artifact kind `image`; widget `image`.
- Input `output_path`: optional destination PNG path; artifact kind `image`; widget `file_save`.
- Output `image_path`: inverted PNG path; artifact kind `image`.
- Output `image`: image artifact metadata; artifact kind `image`.

## Pipeline Usage

Quote or escape the pipe token so the shell passes it to `lfw`:

```bash
lfw run lightflow.text_to_image \
  -i prompt='"a small cat photo"' \
  -i output_path='"out/cat.png"' \
  '|' lightflow.image.invert \
  -i output_path='"out/cat-inverted.png"'
```

The second stage receives the first stage outputs as inputs. Set `output_path`
on the invert stage to avoid writing beside the source image.
