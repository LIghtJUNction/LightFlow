pub(crate) fn serve_usage() -> String {
    [
        "usage:",
        "  lfw serve [--host <host>] [--port <port>]",
        "",
        "Starts the LightFlow HTTP server for workflow catalog, run, node, model, artifact, loop, publish, release, and MCP-adjacent client workflows.",
        "Defaults to 127.0.0.1:5174.",
        "Useful discovery endpoints include /openapi.yaml, /workflows, /nodes, /runs, /models, /loop, /publish, and /release.",
    ]
    .join("\n")
}

pub(crate) fn usage() -> String {
    [
        "usage:",
        "  lfw init [--workflow|--plugin] [path]",
        "  lfw info",
        "  lfw home",
        "  lfw add <crate_name> [--version <version>] [--path <path>|--git <url>] [--package <package>] [--editable] [--global|-g]",
        "  lfw import <path-or-git-url> [--git] [--name <name>] [--global|-g]",
        "  lfw migrate [path]",
        "  lfw new <workflow_id> [--name <name>] [--runtime <capability>] [--global|-g]",
        "  lfw list [--brief|--detail] [--category <name>]",
        "  lfw list --categories",
        "  lfw ls [--brief|--detail] [--category <name>]",
        "  lfw workflows list",
        "  lfw workflows get <workflow_id>",
        "  lfw workflows help <workflow_id>",
        "  lfw workflows deps <workflow_id>",
        "  lfw workflows plan <workflow_id>",
        "  lfw workflows validate <json|-|@file>",
        "  lfw workflows save <json|-|@file>",
        "  lfw deps <workflow_id>",
        "  lfw plan <workflow_id>",
        "  lfw help <workflow_id>",
        "  lfw update [--global|-g]",
        "  lfw upgrade [--global|-g]",
        "  lfw sync [workflow_id] [--model <requirement=variant>] [--hf-model <requirement=format:repo[:file]>] [--hf-url <requirement=url>] [--auto-model|--select-model] [--locked] [--apply]",
        "  lfw models list|requirements|download|rm|prune",
        "  lfw node test <workflow_id>",
        "  lfw mcp [<json|-|@file>]",
        "  lfw batch run <jobs.jsonl> [--workflow <workflow_id>] [--run-id <id>] [--max-gpu-jobs <n|auto>] [--max-cpu-jobs <n|auto>] [--batch-size <n|auto>] [--retries <n>] [--reserve-mem <size>] [--reserve-vram <size>] [--max-load <n>]",
        "  lfw batch resume <run_id> [--max-gpu-jobs <n|auto>]",
        "  lfw trace [last|run_id]",
        "  lfw runs list|get|replay|rm ...",
        "  lfw artifacts [--run <last|run_id>] [--workflow <workflow_id>] [--kind <kind>] [--limit <n>]",
        "  lfw patch list|get|save|validate|rm ...",
        "  lfw replay [last|run_id]",
        "  lfw publish [workflow_id|--crate <path>|--workflows] [--project <name>] [--apply] [--allow-dirty] [--require-publishable]",
        "  lfw dev check [--apply] [--workflow <workflow_id>] [--project <name>]",
        "  lfw dev skill-template <workflow_id> [--write] [--force]",
        "  lfw dev project-config-template [--write] [--force]",
        "  lfw release check [--apply] [--workflow <workflow_id>] [--project <name>]",
        "  lfw loop check [workflow_id]",
        "  lfw loop changes",
        "  lfw loop projects [--dirty] [--project <name>]",
        "  lfw run <workflow_id> [--input|-i <name=json>] [--inputs <json|-|@file>] [--text <text>] [--image <path>] [--output <path>] [--disable <node>] [--enable <node>] [--patch <json|-|@file|name>] ['|' <workflow_id> ...]",
        "  lfw serve [--host <host>] [--port <port>]",
    ]
    .join("\n")
}

pub(crate) fn home_usage() -> String {
    [
        "usage:",
        "  lfw home",
        "",
        "Prints the active LightFlow home paths as JSON.",
        "Fields include home, manifest, workflows, repos, and lfw_path.",
        "Use this to debug global workflow discovery, LFW_PATH, imports, and the default home workspace.",
    ]
    .join("\n")
}

pub(crate) fn workflows_usage() -> String {
    [
        "usage:",
        "  lfw workflows list",
        "  lfw workflows get <workflow_id>",
        "  lfw workflows help <workflow_id>",
        "  lfw workflows deps <workflow_id>",
        "  lfw workflows plan <workflow_id>",
        "  lfw workflows validate <json|-|@file>",
        "  lfw workflows save <json|-|@file>",
        "",
        "Inspects, validates, and saves workflow specs from the active workflow catalog.",
        "Use validate for a dry validation report; use save to write a workflow spec into the local project.",
        "JSON arguments can be inline JSON, '-' for stdin, or '@file' for a file path.",
    ]
    .join("\n")
}

pub(crate) fn workflow_shortcuts_usage() -> String {
    [
        "usage:",
        "  lfw help <workflow_id>",
        "  lfw deps <workflow_id>",
        "  lfw dependencies <workflow_id>",
        "  lfw plan <workflow_id>",
        "",
        "Shortcut commands for inspecting one workflow from the active workflow catalog.",
        "Use help for human-facing workflow guidance, deps for dependency closure and",
        "version mismatches, and plan for the executor/runtime plan before running.",
        "",
        "Equivalent namespaced commands:",
        "  lfw workflows help <workflow_id>",
        "  lfw workflows deps <workflow_id>",
        "  lfw workflows plan <workflow_id>",
    ]
    .join("\n")
}
