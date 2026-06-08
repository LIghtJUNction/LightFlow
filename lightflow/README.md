# LightFlow Project Data

This directory contains versioned project assets for LightFlow.

Use it for self-contained Rust model, node, composition, and workflow assets plus presets, policies, committed fixtures, and schema examples.

Do not split asset metadata into sidecar JSON. A workflow/node/composition/model asset should carry its metadata and definition in the same `.rs` file.

Do not put local run output, credentials, caches, or heavyweight model weights here.

Runtime state uses XDG paths such as `$XDG_STATE_HOME/lightflow`, `$XDG_CACHE_HOME/lightflow`, `$XDG_CONFIG_HOME/lightflow`, and `$XDG_RUNTIME_DIR/lightflow`.
