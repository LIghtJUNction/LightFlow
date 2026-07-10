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

- `tests/standard_text_nodes.rs`
- `tests/standard_image_nodes.rs`
- `tests/standard_control_nodes.rs`
- `tests/standard_model_runtime_nodes.rs`
- `tests/text_to_image_preview.rs`
- `tests/text_to_image_runtime_errors.rs`
- `tests/text_to_image_pipeline.rs`
- `tests/llm_rig.rs`

## ComfyUI API Executor

The generic `comfyui.api.v1` path is verified against deterministic local TCP
mock servers, not a real ComfyUI installation:

```bash
cargo test --test comfyui_runtime
cargo test --test comfyui_runtime_errors
cargo test --test comfyui_runtime_storage
cargo test --test comfyui_nested_replay
```

The tests exercise a generated `lightflow.comfyui.workflow` scaffold through a
real `lfw` subprocess and HTTP sockets. Coverage includes text-to-image node
overrides, image/mask multipart upload bindings, prompt submission, empty then
completed history polling, recursive PNG/GIF/video/audio downloads, output-node
filtering, completed non-file outputs, prompt and execution errors, total
timeout, remote path traversal safety, and replay fingerprints. Same input
content remains stable when ComfyUI returns a different prompt id; changing an
uploaded file produces runtime drift.
Storage coverage also rejects project-root and symlink escapes, streams
multipart bodies, refuses download clobbering, and verifies bounded/redacted
HTTP error bodies. Authorization is sent only to the configured ComfyUI
origin, and the total deadline includes hashing, upload, submit, polling, and
download. Nested replay coverage checks recursive execution nodes, event
depth/node paths/parents, deepest-leaf artifact attribution, and runtime drift.

Server tests verify that long blocking ComfyUI calls do not delay `/health`,
that the shared semaphore caps concurrent blocking runs, and that
`LIGHTFLOW_MAX_BLOCKING_RUNS` defaults to `4` with a valid range of `1..=64`.

This proves the LightFlow protocol implementation and replay evidence. It does
not prove that `127.0.0.1:8188` or another configured endpoint is online, that
an API graph is accepted by installed custom nodes, or that any real model
meets image-quality, VRAM, or performance expectations.

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
