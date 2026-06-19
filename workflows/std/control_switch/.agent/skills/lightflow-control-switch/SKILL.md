---
name: LightFlow Control Switch
description: Use this skill when working with the lightflow.control.switch workflow that selects JSON values by key.
version: 0.1.0
---

# LightFlow Control Switch

Use `lightflow.control.switch` to select a value from a JSON object by selector key.

## Workflow

- Workflow id: `lightflow.control.switch`
- Runtime: `lightflow.control.switch`.
- Input `selector`: required case key.
- Input `cases`: required JSON object.
- Input `default`: optional fallback JSON value.
- Outputs `value` and `selected`.

## Usage

```bash
lfw run lightflow.control.switch -i selector=fast -i cases='{"fast":"draft","final":"polished"}' -i default='"draft"'
```
