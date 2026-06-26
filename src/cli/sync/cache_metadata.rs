use crate::cli::CliResult;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;

pub(super) fn extract_hf_paths(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Object(object) => object
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(|path| vec![path.to_owned()])
            .unwrap_or_default(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| {
                value
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn extract_hf_paths_from_text(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("path: "))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn sha256_file(path: &Path) -> CliResult<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Some(hex_lower(&hasher.finalize())))
}

pub(super) fn file_size(path: &Path) -> CliResult<Option<u64>> {
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(fs::metadata(path)?.len()))
}

pub(super) fn hf_snapshot_revision(path: &Path) -> Option<&str> {
    let mut previous_was_snapshots = false;
    for component in path.components() {
        let text = component.as_os_str().to_str()?;
        if previous_was_snapshots {
            return Some(text);
        }
        previous_was_snapshots = text == "snapshots";
    }
    None
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
