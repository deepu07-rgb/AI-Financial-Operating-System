use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const SETTINGS_FILE_NAME: &str = "ui-settings.json";
const SETTINGS_VERSION: u16 = 1;
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LocalSettings {
    pub version: u16,
    pub theme: ThemeMode,
    pub compact_navigation: bool,
    pub reduce_motion: bool,
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            theme: ThemeMode::System,
            compact_navigation: false,
            reduce_motion: false,
        }
    }
}

impl LocalSettings {
    fn validate(&self) -> Result<(), String> {
        if self.version != SETTINGS_VERSION {
            return Err(format!(
                "Unsupported local settings version: {}",
                self.version
            ));
        }
        Ok(())
    }
}

pub(crate) fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(SETTINGS_FILE_NAME))
        .map_err(|error| format!("Unable to resolve the local settings directory: {error}"))
}

pub(crate) fn restore_settings_if_absent(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    if path.exists() {
        return Ok(false);
    }
    if u64::try_from(bytes.len()).map_err(|_| "Restored settings are too large.".to_owned())?
        > MAX_SETTINGS_BYTES
    {
        return Err("Restored settings exceed the permitted size.".to_owned());
    }
    let settings: LocalSettings = serde_json::from_slice(bytes)
        .map_err(|error| format!("Restored local settings are invalid: {error}"))?;
    settings.validate()?;
    write_settings(path, &settings)?;
    Ok(true)
}

fn read_settings(path: &Path) -> Result<LocalSettings, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(LocalSettings::default()),
        Err(error) => return Err(format!("Unable to inspect local settings: {error}")),
    };

    if metadata.len() > MAX_SETTINGS_BYTES {
        return Err("Local settings exceed the permitted size.".to_string());
    }

    let bytes =
        fs::read(path).map_err(|error| format!("Unable to read local settings: {error}"))?;
    let settings: LocalSettings = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Local settings are invalid: {error}"))?;
    settings.validate()?;
    Ok(settings)
}

fn write_settings(path: &Path, settings: &LocalSettings) -> Result<(), String> {
    settings.validate()?;
    let parent = path
        .parent()
        .ok_or_else(|| "Local settings path has no parent directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create the local settings directory: {error}"))?;

    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("Unable to serialize local settings: {error}"))?;
    let temporary_path = path.with_extension("json.tmp");
    let mut temporary_file = File::create(&temporary_path)
        .map_err(|error| format!("Unable to create the local settings file: {error}"))?;
    temporary_file
        .write_all(&bytes)
        .and_then(|()| temporary_file.sync_all())
        .map_err(|error| format!("Unable to write local settings: {error}"))?;

    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("Unable to replace previous local settings: {error}"))?;
    }
    fs::rename(&temporary_path, path)
        .map_err(|error| format!("Unable to finalize local settings: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn get_local_settings(app: AppHandle) -> Result<LocalSettings, String> {
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || read_settings(&path))
        .await
        .map_err(|error| format!("Local settings task failed: {error}"))?
}

#[tauri::command]
pub async fn save_local_settings(
    app: AppHandle,
    settings: LocalSettings,
) -> Result<LocalSettings, String> {
    settings.validate()?;
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        write_settings(&path, &settings)?;
        Ok(settings)
    })
    .await
    .map_err(|error| format!("Local settings task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("aifos-{name}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn missing_settings_return_safe_defaults() {
        let path = test_directory("missing").join(SETTINGS_FILE_NAME);
        assert_eq!(read_settings(&path), Ok(LocalSettings::default()));
    }

    #[test]
    fn settings_round_trip_with_camel_case_contract() {
        let directory = test_directory("round-trip");
        let path = directory.join(SETTINGS_FILE_NAME);
        let settings = LocalSettings {
            version: 1,
            theme: ThemeMode::Dark,
            compact_navigation: true,
            reduce_motion: true,
        };

        write_settings(&path, &settings).expect("settings should be written");
        assert_eq!(read_settings(&path), Ok(settings));
        let serialized = fs::read_to_string(&path).expect("settings should remain readable");
        assert!(serialized.contains("compactNavigation"));
        assert!(serialized.contains("reduceMotion"));

        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[test]
    fn unsupported_versions_and_fields_are_rejected() {
        let directory = test_directory("invalid");
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join(SETTINGS_FILE_NAME);
        fs::write(
            &path,
            br#"{"version":2,"theme":"system","compactNavigation":false,"reduceMotion":false}"#,
        )
        .expect("invalid settings fixture should be written");

        assert!(read_settings(&path)
            .expect_err("version two should be rejected")
            .contains("Unsupported local settings version"));

        fs::remove_dir_all(directory).expect("test directory should be removable");
    }
}
