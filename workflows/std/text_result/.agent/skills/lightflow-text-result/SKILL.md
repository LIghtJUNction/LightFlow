---
name: LightFlow Text Result
description: Use this skill when working with the lightflow.text_result workflow that normalizes generated text.
version: 0.1.0
---

# LightFlow Text Result

Use `lightflow.text_result` to pass generated text into a final result port.

## Workflow

- Workflow id: `lightflow.text_result`
- Input `text`: generated text.
- Output `result`: final text result.

## Usage

```bash
lfw run lightflow.text_result -i text='"done"'
```
