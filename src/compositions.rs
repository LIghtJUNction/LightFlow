//! Engine support for composition assets.
//!
//! Project composition assets live under `lightflow/compositions/*.rs`.

use crate::asset::{AssetError, AssetKind, AssetRecord, discover_assets};
use std::path::Path;

/// Discover project and built-in composition assets.
pub fn discover(root: &Path) -> Result<Vec<AssetRecord>, AssetError> {
    discover_assets(root, AssetKind::Composition)
}
