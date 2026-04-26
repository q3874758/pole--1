use std::fs;
use std::io;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub fn load_json<T, E>(path: impl AsRef<Path>) -> Result<T, E>
where
    T: DeserializeOwned,
    E: From<io::Error> + From<serde_json::Error>,
{
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_json_or_default<T, E>(path: impl AsRef<Path>) -> Result<T, E>
where
    T: Default + DeserializeOwned,
    E: From<io::Error> + From<serde_json::Error>,
{
    let path = path.as_ref();
    if !path.exists() {
        return Ok(T::default());
    }
    load_json(path)
}

pub fn save_pretty_json<T, E>(value: &T, path: impl AsRef<Path>) -> Result<(), E>
where
    T: Serialize,
    E: From<io::Error> + From<serde_json::Error>,
{
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, content)?;
    Ok(())
}
