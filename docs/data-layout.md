# Data Layout

LightFlow project files are ordinary source-controlled files under `lightflow/`.

```text
lightflow/
  components/
    <component_id>.json
  workflows/
    <workflow_id>.json
```

## Component Files

Component files describe reusable leaf units:

```json
{
  "id": "component.text_prompt",
  "name": "Text Prompt",
  "inputs": [{ "name": "value", "type": "json" }],
  "outputs": [{ "name": "prompt", "type": "text" }]
}
```

## Workflow Files

Workflow files describe directed graphs. Nodes can reference either components
or workflows:

```json
{
  "id": "workflow.example",
  "name": "Example",
  "inputs": [{ "name": "value", "type": "json" }],
  "outputs": [{ "name": "text", "type": "text" }],
  "nodes": [
    {
      "id": "prompt",
      "uses": "component",
      "component_id": "component.text_prompt"
    },
    {
      "id": "nested",
      "uses": "workflow",
      "workflow_id": "workflow.other"
    }
  ],
  "edges": []
}
```

## Not Stored Here

Do not commit runtime state, credentials, generated artifacts, caches, or model
weights under `lightflow/`.
