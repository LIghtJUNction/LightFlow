//! Engine support for model assets.
//!
//! Project model assets live under `lightflow/models/*.rs`.

use crate::asset::{AssetError, AssetKind, AssetRecord, discover_assets};
use std::path::Path;

/// Discover project model assets.
pub fn discover(root: &Path) -> Result<Vec<AssetRecord>, AssetError> {
    discover_assets(root, AssetKind::Model)
}
