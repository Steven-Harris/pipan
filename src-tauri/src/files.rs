//! File attachment reader for drag-and-drop and file picker workflows in the chat composer.
//!
//! * Image files (`png`, `jpg`, `jpeg`, `gif`, `webp`, `bmp`) are read and base64-encoded
//!   into `{ type: "image", data, mimeType }` for Pi RPC `prompt` / `steer` / `follow_up`.
//! * Code, text, and other files return workspace-relative path metadata and optional
//!   UTF-8 text content for inline fenced blocks when dropped from outside the workspace.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub use crate::fs_explorer::{
    WorkspaceFileEntry, WorkspaceFilePreview, get_git_file_statuses, list_workspace_files,
    read_workspace_file,
};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024; // 20 MiB
const MAX_INLINE_TEXT_BYTES: u64 = 256 * 1024; // 256 KiB

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileAttachmentPayload {
    /// `"image"` | `"file"`
    pub kind: String,
    /// `"image"` for images (matching Pi RPC image entry `type`), `"file"` otherwise.
    #[serde(rename = "type")]
    pub attachment_type: String,
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    pub inside_workspace: bool,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard RFC 4648 Base64 encoder.
pub fn encode_base64(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    let (chunks, rem) = input.as_chunks::<3>();
    for &[b0, b1, b2] in chunks {
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize]);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize]);
        out.push(BASE64_ALPHABET[((n >> 6) & 0x3f) as usize]);
        out.push(BASE64_ALPHABET[(n & 0x3f) as usize]);
    }
    if rem.len() == 1 {
        let n = (rem[0] as u32) << 16;
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize]);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize]);
        out.push(b'=');
        out.push(b'=');
    } else if rem.len() == 2 {
        let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize]);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize]);
        out.push(BASE64_ALPHABET[((n >> 6) & 0x3f) as usize]);
        out.push(b'=');
    }
    String::from_utf8(out).expect("base64 alphabet is valid ASCII")
}

pub fn image_mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext.trim().to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn clean_verbatim_prefix(raw: &str) -> String {
    raw.trim_start_matches(r"\\?\").replace('\\', "/")
}

fn compute_workspace_relative(file_path: &Path, cwd: Option<&str>) -> (Option<String>, bool) {
    let Some(cwd_raw) = cwd.map(str::trim).filter(|s| !s.is_empty()) else {
        return (None, false);
    };
    let cwd_path = PathBuf::from(cwd_raw);

    // Try canonical comparison first, falling back to lexical strip_prefix
    let file_canon = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
    let cwd_canon = std::fs::canonicalize(&cwd_path).unwrap_or_else(|_| cwd_path.clone());

    if let Ok(rel) = file_canon.strip_prefix(&cwd_canon) {
        let rel_str = clean_verbatim_prefix(&rel.to_string_lossy());
        if !rel_str.is_empty() {
            return (Some(rel_str), true);
        }
    }

    if let Ok(rel) = file_path.strip_prefix(&cwd_path) {
        let rel_str = clean_verbatim_prefix(&rel.to_string_lossy());
        if !rel_str.is_empty() {
            return (Some(rel_str), true);
        }
    }

    (None, false)
}

pub fn read_file_attachment_sync(
    path: &Path,
    cwd: Option<&str>,
) -> Result<FileAttachmentPayload, String> {
    if !path.exists() {
        return Err(format!(
            "Attachment file does not exist: {}",
            path.display()
        ));
    }
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read metadata for {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "Attachment path is not a regular file: {}",
            path.display()
        ));
    }

    let size_bytes = meta.len();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let (relative_path, inside_workspace) = compute_workspace_relative(path, cwd);
    let normalized_path = path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    if let Some(image_mime) = image_mime_for_extension(&ext) {
        if size_bytes > MAX_IMAGE_BYTES {
            return Err(format!(
                "Image '{}' exceeds maximum attachment size of 20 MB",
                name
            ));
        }
        let bytes = std::fs::read(path)
            .map_err(|e| format!("Failed to read image file '{}': {e}", name))?;
        let data = encode_base64(&bytes);
        return Ok(FileAttachmentPayload {
            kind: "image".to_string(),
            attachment_type: "image".to_string(),
            name,
            path: normalized_path,
            relative_path,
            inside_workspace,
            mime_type: image_mime.to_string(),
            size_bytes,
            data: Some(data),
            text_content: None,
        });
    }

    // Non-image code / text / other file
    let text_content = if size_bytes <= MAX_INLINE_TEXT_BYTES {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };

    Ok(FileAttachmentPayload {
        kind: "file".to_string(),
        attachment_type: "file".to_string(),
        name,
        path: normalized_path,
        relative_path,
        inside_workspace,
        mime_type: "text/plain".to_string(),
        size_bytes,
        data: None,
        text_content,
    })
}

#[tauri::command]
pub async fn read_file_attachment(
    path: String,
    cwd: Option<String>,
) -> Result<FileAttachmentPayload, String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path.trim());
        read_file_attachment_sync(&p, cwd.as_deref())
    })
    .await
    .map_err(|e| format!("Failed to join read_file_attachment task: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pipan-files-test-{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn test_encode_base64_rfc4648_vectors() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_read_image_attachment_encodes_base64_and_mime() {
        let dir = make_temp_dir("image");
        let img_path = dir.join("diagram.png");
        let raw_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        std::fs::write(&img_path, raw_bytes).unwrap();

        let att = read_file_attachment_sync(&img_path, Some(&dir.to_string_lossy())).unwrap();
        assert_eq!(att.kind, "image");
        assert_eq!(att.attachment_type, "image");
        assert_eq!(att.mime_type, "image/png");
        assert_eq!(att.name, "diagram.png");
        assert!(att.inside_workspace);
        assert_eq!(att.relative_path.as_deref(), Some("diagram.png"));
        assert_eq!(att.data.as_deref(), Some(encode_base64(raw_bytes).as_str()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_code_attachment_inside_and_outside_workspace() {
        let root = make_temp_dir("code");
        let ws = root.join("workspace");
        let outside = root.join("outside");
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let inside_file = ws.join("src").join("main.rs");
        let outside_file = outside.join("notes.md");
        std::fs::write(&inside_file, "fn main() {}\n").unwrap();
        std::fs::write(&outside_file, "External context note\n").unwrap();

        let ws_str = ws.to_string_lossy().to_string();

        let inside_att = read_file_attachment_sync(&inside_file, Some(&ws_str)).unwrap();
        assert_eq!(inside_att.kind, "file");
        assert!(inside_att.inside_workspace);
        assert_eq!(inside_att.relative_path.as_deref(), Some("src/main.rs"));
        assert_eq!(inside_att.text_content.as_deref(), Some("fn main() {}\n"));

        let outside_att = read_file_attachment_sync(&outside_file, Some(&ws_str)).unwrap();
        assert_eq!(outside_att.kind, "file");
        assert!(!outside_att.inside_workspace);
        assert!(outside_att.relative_path.is_none());
        assert_eq!(
            outside_att.text_content.as_deref(),
            Some("External context note\n")
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
