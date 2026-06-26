use crate::cli::{CliError, CliResult};
use crate::workflow::{ModelProvider, ModelVariant};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct CustomHfModel {
    pub(super) format: String,
    pub(super) repo: String,
    pub(super) file: Option<String>,
}

pub(super) fn parse_custom_hf_model(value: &str, flag: &str) -> CliResult<(String, CustomHfModel)> {
    let Some((requirement, spec)) = value.split_once('=') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=format:repo[:file]>"
        )));
    };
    let Some((format, location)) = spec.split_once(':') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=format:repo[:file]>"
        )));
    };
    let (repo, file) = parse_hf_location(location)?;
    if requirement.is_empty() || format.is_empty() || repo.is_empty() {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=format:repo[:file]>"
        )));
    }
    Ok((
        requirement.to_owned(),
        CustomHfModel {
            format: format.to_owned(),
            repo: repo.to_owned(),
            file,
        },
    ))
}

pub(super) fn parse_custom_hf_url(value: &str, flag: &str) -> CliResult<(String, CustomHfModel)> {
    let Some((requirement, url)) = value.split_once('=') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=url>"
        )));
    };
    let (repo, file) = parse_hf_url(url)?;
    let format = file
        .as_deref()
        .and_then(infer_model_format)
        .unwrap_or("custom")
        .to_owned();
    Ok((requirement.to_owned(), CustomHfModel { format, repo, file }))
}

fn parse_hf_location(location: &str) -> CliResult<(String, Option<String>)> {
    if location.starts_with("https://huggingface.co/")
        || location.starts_with("http://huggingface.co/")
    {
        return parse_hf_url(location);
    }
    let (repo, file) = match location.split_once(':') {
        Some((repo, file)) => (
            repo.to_owned(),
            Some(file.to_owned()).filter(|file| !file.is_empty()),
        ),
        None => (location.to_owned(), None),
    };
    Ok((repo, file))
}

pub(super) fn parse_hf_url(url: &str) -> CliResult<(String, Option<String>)> {
    let path = url
        .strip_prefix("https://huggingface.co/")
        .or_else(|| url.strip_prefix("http://huggingface.co/"))
        .ok_or_else(|| CliError::Usage(format!("unsupported Hugging Face URL: {url}")))?;
    if let Some((repo, rest)) = path
        .split_once("/resolve/")
        .or_else(|| path.split_once("/blob/"))
    {
        let file = rest
            .split_once('/')
            .map(|(_, file)| file)
            .filter(|file| !file.is_empty())
            .ok_or_else(|| {
                CliError::Usage(format!(
                    "Hugging Face file URL is missing a filename: {url}"
                ))
            })?;
        return Ok((repo.to_owned(), Some(file.to_owned())));
    }
    Ok((path.trim_end_matches('/').to_owned(), None))
}

pub(super) fn infer_model_format(file: &str) -> Option<&'static str> {
    let lower = file.to_ascii_lowercase();
    if lower.ends_with(".gguf") {
        Some("gguf")
    } else if lower.ends_with(".safetensors") {
        Some("safetensors")
    } else if lower.ends_with(".bin") {
        Some("bin")
    } else {
        None
    }
}

pub(super) fn available_variant_summary(model: &crate::workflow::ModelRequirement) -> String {
    if model.variants.is_empty() {
        return "none declared".to_owned();
    }
    model
        .variants
        .iter()
        .map(|variant| {
            format!(
                "{} ({}, {})",
                variant.id,
                variant.format,
                model_download_url(variant).unwrap_or_else(|| format!(
                    "{}:{}",
                    variant.provider.as_str(),
                    variant.repo
                ))
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn model_download_url(variant: &ModelVariant) -> Option<String> {
    match variant.provider {
        ModelProvider::HuggingFace => Some(hf_download_url(&variant.repo, variant.file.as_deref())),
    }
}

pub(super) fn hf_download_url(repo: &str, file: Option<&str>) -> String {
    match file {
        Some(file) => format!(
            "https://huggingface.co/{repo}/resolve/main/{}",
            percent_encode_hf_path(file)
        ),
        None => format!("https://huggingface.co/{repo}"),
    }
}

fn percent_encode_hf_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
