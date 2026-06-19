---
name: LightFlow JSON Extract
description: Use this skill when working with the lightflow.json.extract workflow that extracts JSON values by dot path.
version: 0.1.0
---

# LightFlow JSON Extract

Use `lightflow.json.extract` to extract a JSON value by dot path. Array indexes are numeric path segments.

## Workflow

- Workflow id: `lightflow.json.extract`
- Runtime: `lightflow.json.extract`.
- Input `value`: required source JSON; widget `json`.
- Input `path`: required dot path, for example `user.name` or `items.0.title`.
- Outputs `value`, `text`, and `found`.

## Usage

```bash
lfw run lightflow.json.extract -i value='{"user":{"name":"Ada"}}' -i path=user.name
```
