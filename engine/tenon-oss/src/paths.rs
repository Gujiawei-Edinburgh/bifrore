use std::env;
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf, String> {
    Ok(tenon_home()?.join("config.json"))
}

pub fn log_dir() -> Result<PathBuf, String> {
    Ok(tenon_home()?.join("log"))
}

pub fn expand_user_path(path: &str) -> Result<PathBuf, String> {
    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(user_home()?.join(rest));
    }
    Ok(PathBuf::from(path))
}

fn tenon_home() -> Result<PathBuf, String> {
    configured_tenon_home().map(Ok).unwrap_or_else(default_tenon_home)
}

fn configured_tenon_home() -> Option<PathBuf> {
    env::var_os("TENON_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn default_tenon_home() -> Result<PathBuf, String> {
    Ok(user_home()?.join(".tenon"))
}

fn user_home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}
