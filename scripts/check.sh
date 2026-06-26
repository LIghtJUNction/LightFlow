#!/usr/bin/env sh
set -eu

usage() {
  cat <<'USAGE'
usage:
  scripts/check.sh [--list] [--full] [--project <name>] [--workflow <workflow_id>]

Runs the local developer verification gates.

Default mode is the fast handoff loop:
  cargo fmt --check
  cargo check
  cargo test project_setup_commands_are_stable_and_deduped
  cargo test project_set_config_matches_git_submodules
  cargo test --test standard_nodes repository_workflow_crates_have_agent_skills
  cargo test lfw_help_advertises_project_scoped_developer_release_and_publish_selectors
  cargo test publish_endpoint_can_filter_project_workspaces
  cargo test mcp_exposes_backend_tools

--full also runs:
  cargo run --bin lfw -- publish --workflows --require-publishable [--project <name>]
  cargo run --bin lfw -- loop projects --dirty [--project <name>]
  cargo run --bin lfw -- dev check
  cargo run --bin lfw -- release check
  cargo clippy --all-targets -- -D warnings
  cargo test
  cargo test --features rig --test llm_rig
  cargo check --features flux-native

Use --project and --workflow with --full to scope dev/release review, for
example:
  scripts/check.sh --list --full --project lightflow-std
  scripts/check.sh --full --project lightflow-std
  scripts/check.sh --full --workflow lightflow.text_plan
  scripts/check.sh --full --project lightflow-std --workflow lightflow.text_plan
USAGE
}

mode=fast
project=
workflow=
list_only=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    "--list")
      list_only=true
      shift
      ;;
    "--full")
      mode=full
      shift
      ;;
    "--project")
      if [ "$#" -lt 2 ]; then
        usage >&2
        exit 2
      fi
      project=$2
      shift 2
      ;;
    "--workflow")
      if [ "$#" -lt 2 ]; then
        usage >&2
        exit 2
      fi
      workflow=$2
      shift 2
      ;;
    "-h"|"--help")
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

if [ -n "$project" ] && [ "$mode" != "full" ]; then
  printf '%s\n' "scripts/check.sh: --project requires --full" >&2
  exit 2
fi

if [ -n "$workflow" ] && [ "$mode" != "full" ]; then
  printf '%s\n' "scripts/check.sh: --workflow requires --full" >&2
  exit 2
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

run() {
  printf '+'
  for arg in "$@"; do
    printf ' %s' "$arg"
  done
  printf '\n'
  if [ "$list_only" = false ]; then
    "$@"
  fi
}

run cargo fmt --check
run cargo check
run cargo test project_setup_commands_are_stable_and_deduped
run cargo test project_set_config_matches_git_submodules
run cargo test --test standard_nodes repository_workflow_crates_have_agent_skills
run cargo test lfw_help_advertises_project_scoped_developer_release_and_publish_selectors
run cargo test publish_endpoint_can_filter_project_workspaces
run cargo test mcp_exposes_backend_tools

if [ "$mode" = "full" ]; then
  set -- publish --workflows --require-publishable
  if [ -n "$project" ]; then
    set -- "$@" --project "$project"
  fi
  run cargo run --bin lfw -- "$@"

  set -- loop projects --dirty
  if [ -n "$project" ]; then
    set -- "$@" --project "$project"
  fi
  run cargo run --bin lfw -- "$@"

  if [ -n "$project" ] || [ -n "$workflow" ]; then
    set -- dev check
    if [ -n "$project" ]; then
      set -- "$@" --project "$project"
    fi
    if [ -n "$workflow" ]; then
      set -- "$@" --workflow "$workflow"
    fi
    run cargo run --bin lfw -- "$@"

    set -- release check
    if [ -n "$project" ]; then
      set -- "$@" --project "$project"
    fi
    if [ -n "$workflow" ]; then
      set -- "$@" --workflow "$workflow"
    fi
    run cargo run --bin lfw -- "$@"
  else
    run cargo run --bin lfw -- dev check
    run cargo run --bin lfw -- release check
  fi

  run cargo clippy --all-targets -- -D warnings
  run cargo test
  run cargo test --features rig --test llm_rig
  run cargo check --features flux-native
fi
