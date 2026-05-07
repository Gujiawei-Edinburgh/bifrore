use std::env;
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf, String> {
    Ok(tenon_home()?.join("config.json"))
}

pub fn log_dir() -> Result<PathBuf, String> {
    Ok(tenon_home()?.join("log"))
}

fn tenon_home() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("TENON_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".tenon"))
        .ok_or_else(|| "HOME is not set".to_string())
}
