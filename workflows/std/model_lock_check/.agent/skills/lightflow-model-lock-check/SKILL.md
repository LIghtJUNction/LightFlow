---
name: LightFlow Model Lock Check
description: Use this skill when working with the lightflow.model.lock_check workflow that checks lfw.lock model entries.
version: 0.1.0
---

# LightFlow Model Lock Check

Use `lightflow.model.lock_check` to check whether `lfw.lock` contains a model entry for `<workflow_id>::<requirement_id>`.

## Workflow

- Workflow id: `lightflow.model.lock_check`
- Runtime: `lightflow.model.lock.check`.
- Inputs `workflow_id` and `requirement_id`: required text values.
- Outputs `locked`, `exists`, `path`, and `entry`.

## Usage

```bash
lfw run lightflow.model.lock_check -i workflow_id=lightflow.text_to_image -i requirement_id=image_model
```
