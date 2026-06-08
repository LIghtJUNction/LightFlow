//! Engine support for node assets.
//!
//! Project node assets live under `lightflow/nodes/*.rs`.

use crate::asset::{AssetError, AssetKind, AssetRecord, discover_assets};
use std::path::Path;

/// Discover project and built-in node assets.
pub fn discover(root: &Path) -> Result<Vec<AssetRecord>, AssetError> {
    discover_assets(root, AssetKind::Node)
}
