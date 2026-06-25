# LightFlow Project Set

This folder groups related LightFlow workflow projects for local iteration:

- `lightflow-flux`
- `lightflow-std`
- `lightflow-rig`
- `lightflow-auto-editing`

The first three entries are symlinks to sibling checkouts under
`/home/lightjunction/Documents/GITHUB`. `lightflow-auto-editing` is a git
submodule managed inside this folder. The projects remain independent git
repositories; edit, commit, and push changes from each project directory.

Use this folder when you want one workspace view across the core LightFlow repo
and the workflow projects that are commonly iterated together.

Inspect the current linked workspace set with:

```bash
lfw loop projects
```

The same read-only catalog is exposed through `GET /loop/projects`,
`lightflow.loop.projects`, and `lightflow://loop/projects`.
