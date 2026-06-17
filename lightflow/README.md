# LightFlow Project Files

LightFlow currently has two source-controlled concepts:

- `components/`: reusable leaf building blocks.
- `workflows/`: directed graphs that can call components or nest other workflows.

Runtime state, credentials, caches, generated artifacts, and heavyweight model
weights do not belong here.
