# Runtime Verification

This document records the v0.2 runtime verification paths. It distinguishes
deterministic local/runtime-contract checks from real model quality checks.

## Preview And Mock Executors

Preview image executors and mock LLM executors are deterministic local paths for
tests, demos, and UI development. They verify LightFlow workflow plumbing,
artifact handling, node cards, and run history, but they do not prove production
model quality.

Verified with:

```bash
cargo test
```

Relevant coverage:

- `tests/standard_nodes.rs`
- `tests/text_to_image.rs`
- `tests/llm_rig.rs`

## RIG LLM Runtime

Provider-backed LLM execution is gated by `--features rig`.

Verified local paths:

```bash
cargo test --features rig --test llm_rig
```

This covers:

- deterministic `provider="mock"` execution;
- configured OpenAI-compatible execution through a local HTTP endpoint using
  `provider="openai-compatible"`, `api_key`, and `base_url`.

For a real external provider smoke test, run a workflow that declares
`lightflow.llm.generate` with:

```bash
cargo run --features rig --bin lfw -- run <workflow_id> \
  -i provider='"openai-compatible"' \
  -i model='"your-model"' \
  -i api_key='"your-key"' \
  -i base_url='"https://your-openai-compatible-endpoint/v1"' \
  -i prompt='"hello"'
```

The RIG adapter also supports provider names documented in `README.md`.

## FLUX External Runner

The external FLUX path is selected when `LIGHTFLOW_FLUX_RUNNER` points to an
executable. LightFlow passes the task, prompt, source image and mask paths,
sampling settings, output path, and locked model paths to that runner.

Verified with:

```bash
cargo test --test text_to_image
```

Relevant coverage:

- `flux_text_to_image_uses_external_runner_contract`
- `flux_edit_and_inpaint_use_external_runner_contracts`

## Native FLUX

Native FLUX is gated by `--features flux-native`.

Verified local build:

```bash
cargo check --features flux-native
```

The local verification completed successfully for the default CPU native build.
The vendored `diffusion-rs-sys` build emitted warnings from generated bindgen
output; LightFlow itself checked successfully.

Build prerequisites:

- CMake and a working C/C++ toolchain.
- Bindgen-compatible Clang/libclang when bindings need regeneration; bundled
  bindings are used as fallback when generation fails.
- A C++ standard library for the target platform.

Supported platform notes from the current build script:

- Linux CPU builds use the default CMake/native toolchain path.
- Apple targets link `Accelerate` and `Foundation`, and enable Metal unless
  Vulkan is explicitly selected.
- `flux-native-cuda` requires CUDA libraries. On non-Windows targets the build
  script searches `/usr/local/cuda/lib64`, `/usr/local/cuda/lib64/stubs`,
  `/opt/cuda/lib64`, and `/opt/cuda/lib64/stubs`; `CUDA_COMPUTE_CAP` can set
  the CUDA compute target.
- `flux-native-vulkan` requires Vulkan libraries. On Windows `VULKAN_SDK` must
  be set; on Unix-like targets the build links `vulkan` and optionally searches
  `$VULKAN_SDK/lib`.

Real image quality and performance still depend on the selected model files,
hardware backend, and runtime memory budget.
