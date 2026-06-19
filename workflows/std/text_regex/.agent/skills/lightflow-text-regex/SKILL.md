---
name: LightFlow Text Regex
description: Use this skill when working with the lightflow.text.regex workflow that matches or replaces text with regular expressions.
version: 0.1.0
---

# LightFlow Text Regex

Use `lightflow.text.regex` to run deterministic regex matching or replacement in a workflow.

## Workflow

- Workflow id: `lightflow.text.regex`
- Runtime: `lightflow.text.regex`.
- Input `text`: required source text; widget `textarea`.
- Input `pattern`: required Rust regex pattern.
- Input `replacement`: optional replacement text. When omitted, output `text` is the original input.
- Outputs: `text`, `matched`, `match_count`, `captures`, `first_match`.

## Usage

```bash
lfw run lightflow.text.regex -i text='cat 42' -i pattern='(\d+)' -i replacement='id:$1'
```
