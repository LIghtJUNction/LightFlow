//! Engine support for workflow assets.
//!
//! Project workflow assets live under `lightflow/workflows/*.rs`.

use crate::asset::{AssetError, AssetKind, AssetRecord, discover_assets};
use std::path::Path;

/// Discover project and built-in workflow assets.
pub fn discover(root: &Path) -> Result<Vec<AssetRecord>, AssetError> {
    discover_assets(root, AssetKind::Workflow)
}
