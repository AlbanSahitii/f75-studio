//! App preferences in settings.json in the app-data folder. (Keyboard settings live on the keyboard.)
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Draw the editor's keyboard with the ISO layout instead of ANSI.
    pub iso: bool,
    /// Open at Login was switched on once, as the default; after that it is the user's choice.
    pub login_default_applied: bool,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// The saved preferences; a missing or damaged file means the defaults.
pub fn load(app: &AppHandle) -> Settings {
    path(app).ok().and_then(|p| fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: Settings) -> Result<(), String> {
    let path = path(app)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(&settings).expect("plain data serializes"))
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| format!("couldn't save settings: {e}"))
}
