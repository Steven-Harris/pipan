//! Extension channel shim loader.
//!
//! PiPan ships `resources/pipan-channel-shim.ts`, a Pi extension that carries the extension
//! channel over Pi's RPC (see `docs/plans/extension-channel-spec.md`). Rust only has to write the
//! file and pass `--extension <path>` to the primary Pi session; all decoding happens in the
//! webview.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

pub const CHANNEL_SHIM_FILE_NAME: &str = "pipan-channel-shim.ts";
pub const CHANNEL_SHIM_SOURCE: &str = include_str!("../resources/pipan-channel-shim.ts");

/// Writes the shim into `dir` (created if missing) and returns its path. The file is rewritten
/// every time so a newer PiPan build always ships its own shim.
pub(crate) fn write_channel_shim(dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create extension dir: {}", e))?;
    let path = dir.join(CHANNEL_SHIM_FILE_NAME);
    fs::write(&path, CHANNEL_SHIM_SOURCE)
        .map_err(|e| format!("Failed to write channel shim extension: {}", e))?;
    Ok(path)
}

/// Ensures the bundled channel shim exists in the app data directory and returns its path.
pub fn ensure_channel_shim(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?
        .join("extensions");
    write_channel_shim(&dir)
}

/// The Pi CLI arguments that load the shim.
pub(crate) fn channel_shim_args(shim_path: &Path) -> Vec<OsString> {
    vec![
        OsString::from("--extension"),
        shim_path.as_os_str().to_os_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_embedded_shim_and_overwrites_stale_content() {
        let dir = std::env::temp_dir().join(format!("pipan-channel-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let nested = dir.join("extensions");

        let path = write_channel_shim(&nested).expect("write shim");
        assert_eq!(path, nested.join(CHANNEL_SHIM_FILE_NAME));
        assert_eq!(fs::read_to_string(&path).unwrap(), CHANNEL_SHIM_SOURCE);

        fs::write(&path, "stale").unwrap();
        write_channel_shim(&nested).expect("rewrite shim");
        assert_eq!(fs::read_to_string(&path).unwrap(), CHANNEL_SHIM_SOURCE);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shim_args_pass_the_path_after_extension_flag() {
        let args = channel_shim_args(Path::new("/tmp/x/pipan-channel-shim.ts"));
        assert_eq!(
            args,
            vec![
                OsString::from("--extension"),
                OsString::from("/tmp/x/pipan-channel-shim.ts")
            ]
        );
    }

    #[test]
    fn shim_source_declares_the_announce_widget() {
        assert!(CHANNEL_SHIM_SOURCE.contains("pipan:shim"));
    }
}
