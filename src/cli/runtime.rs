use super::project::workflow_collection_manifest;
use super::{CliError, CliResult};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct RuntimeConfig {
    pub(super) rc_path: PathBuf,
    pub(super) lfw_path: String,
    pub(super) workflow_paths: Vec<PathBuf>,
    pub(super) home_path: PathBuf,
    pub(super) default_workflow_path: PathBuf,
}

impl RuntimeConfig {
    pub(super) fn load() -> CliResult<Self> {
        let config_home = xdg_config_home()?;
        let data_home = xdg_data_home()?;
        let rc_path = config_home.join("lightflow").join(".lfwrc");
        let home_path = data_home.join("lightflow");
        let default_workflow_path = home_path.join("workflows");
        let raw_lfw_path = env::var("LFW_PATH")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| home_path.display().to_string());
        let lfw_path =
            normalize_default_lfw_path(&raw_lfw_path, &home_path, &default_workflow_path);
        let workflow_paths = workflow_search_paths(&lfw_path, &home_path, &default_workflow_path);
        Ok(Self {
            rc_path,
            lfw_path,
            workflow_paths,
            home_path,
            default_workflow_path,
        })
    }
}

fn normalize_default_lfw_path(
    lfw_path: &str,
    home_path: &Path,
    default_workflow_path: &Path,
) -> String {
    let paths = env::split_paths(lfw_path)
        .map(|path| {
            if path == default_workflow_path {
                home_path.to_path_buf()
            } else {
                path
            }
        })
        .collect::<Vec<_>>();
    env::join_paths(paths)
        .ok()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| lfw_path.to_owned())
}

fn workflow_search_paths(
    lfw_path: &str,
    home_path: &Path,
    default_workflow_path: &Path,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for path in env::split_paths(lfw_path) {
        push_unique_path(&mut paths, path);
    }
    push_unique_path(&mut paths, home_path.to_path_buf());
    push_unique_path(&mut paths, default_workflow_path.to_path_buf());
    paths
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.as_os_str().is_empty() || paths.iter().any(|existing| existing == &path) {
        return;
    }
    paths.push(path);
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ShellSetup {
    pub(super) rc_created: bool,
    pub(super) workspace_manifest: PathBuf,
    pub(super) workspace_created: bool,
    pub(super) shell: Option<&'static str>,
    pub(super) shell_config: Option<PathBuf>,
    pub(super) source_line: Option<String>,
    pub(super) source_installed: bool,
}

pub(super) fn ensure_lfw_shell_setup(runtime: &RuntimeConfig) -> CliResult<ShellSetup> {
    fs::create_dir_all(&runtime.default_workflow_path)?;
    let (workspace_manifest, workspace_created) =
        ensure_default_workflow_workspace(&runtime.home_path)?;
    let parent = runtime
        .rc_path
        .parent()
        .ok_or_else(|| CliError::Usage("invalid LightFlow rc path".to_owned()))?;
    fs::create_dir_all(parent)?;

    let shell = detect_shell();
    let rc_created = if runtime.rc_path.exists() {
        false
    } else {
        fs::write(&runtime.rc_path, lfwrc_body(shell, runtime))?;
        true
    };
    let Some(shell) = shell else {
        return Ok(ShellSetup {
            rc_created,
            workspace_manifest,
            workspace_created,
            shell: None,
            shell_config: None,
            source_line: None,
            source_installed: false,
        });
    };
    let Some(shell_config) = shell_config_path(shell)? else {
        return Ok(ShellSetup {
            rc_created,
            workspace_manifest,
            workspace_created,
            shell: Some(shell),
            shell_config: None,
            source_line: None,
            source_installed: false,
        });
    };
    let source_line = source_line(shell, &runtime.rc_path);
    let source_installed = ensure_source_line(&shell_config, &source_line)?;
    Ok(ShellSetup {
        rc_created,
        workspace_manifest,
        workspace_created,
        shell: Some(shell),
        shell_config: Some(shell_config),
        source_line: Some(source_line),
        source_installed,
    })
}

fn ensure_default_workflow_workspace(path: &Path) -> CliResult<(PathBuf, bool)> {
    let manifest = path.join("Cargo.toml");
    if manifest.exists() {
        return Ok((manifest, false));
    }
    fs::write(&manifest, workflow_collection_manifest())?;
    Ok((manifest, true))
}

fn lfwrc_body(shell: Option<&str>, runtime: &RuntimeConfig) -> String {
    let value = runtime.home_path.display().to_string();
    if shell == Some("fish") {
        format!(
            "# LightFlow CLI configuration\nset -gx LFW_PATH {}\n",
            fish_quote(&value)
        )
    } else {
        format!(
            "# LightFlow CLI configuration\nexport LFW_PATH={}\n",
            shell_quote(&value)
        )
    }
}

fn detect_shell() -> Option<&'static str> {
    let shell = env::var("SHELL").ok()?;
    let name = Path::new(&shell).file_name()?.to_str()?;
    match name {
        "bash" => Some("bash"),
        "zsh" => Some("zsh"),
        "fish" => Some("fish"),
        _ => None,
    }
}

fn shell_config_path(shell: &str) -> CliResult<Option<PathBuf>> {
    match shell {
        "bash" => Ok(env::var_os("HOME").map(|home| PathBuf::from(home).join(".bashrc"))),
        "zsh" => {
            if let Some(zdotdir) = env::var_os("ZDOTDIR") {
                Ok(Some(PathBuf::from(zdotdir).join(".zshrc")))
            } else {
                Ok(env::var_os("HOME").map(|home| PathBuf::from(home).join(".zshrc")))
            }
        }
        "fish" => Ok(Some(xdg_config_home()?.join("fish").join("config.fish"))),
        _ => Ok(None),
    }
}

fn source_line(shell: &str, rc_path: &Path) -> String {
    let rc = rc_path.display().to_string();
    match shell {
        "fish" => format!("source {}", fish_quote(&rc)),
        _ => format!("source {}", shell_quote(&rc)),
    }
}

fn ensure_source_line(path: &Path, source_line: &str) -> CliResult<bool> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(CliError::Io(error)),
    };
    if existing.lines().any(|line| line.trim() == source_line) {
        return Ok(false);
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str("\n# LightFlow\n");
    next.push_str(source_line);
    next.push('\n');
    fs::write(path, next)?;
    Ok(true)
}

fn xdg_config_home() -> CliResult<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| CliError::Usage("HOME is required to locate XDG_CONFIG_HOME".to_owned()))
}

fn xdg_data_home() -> CliResult<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| CliError::Usage("HOME is required to locate XDG_DATA_HOME".to_owned()))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn fish_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}
