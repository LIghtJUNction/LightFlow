use super::super::ApiResult;
use super::super::publish_readiness::read_publish_workspace_document;
use crate::api::{cargo_manifest_api_error, read_cargo_manifest};
use std::collections::{BTreeMap, btree_map::Entry};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub(super) fn cached_workspace_document<'a>(
    workspace_documents: &'a mut BTreeMap<PathBuf, Option<DocumentMut>>,
    workspace_root: &Path,
) -> ApiResult<&'a Option<DocumentMut>> {
    let document = match workspace_documents.entry(workspace_root.to_path_buf()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert(read_publish_workspace_document(workspace_root)?),
    };
    Ok(document)
}

pub(super) fn cached_manifest_document<'a>(
    manifest_documents: &'a mut BTreeMap<PathBuf, DocumentMut>,
    manifest: &Path,
) -> ApiResult<&'a DocumentMut> {
    let document = match manifest_documents.entry(manifest.to_path_buf()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            entry.insert(read_cargo_manifest(manifest).map_err(cargo_manifest_api_error)?)
        }
    };
    Ok(document)
}
