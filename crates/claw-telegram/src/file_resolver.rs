//! Telegram file resolver — downloads files from Telegram Bot API.
//!
//! Resolves `tg://file/{file_id}` pseudo-URLs into real downloaded files
//! on the local filesystem. Called after normalization, before dispatch.

use std::path::{Path, PathBuf};

use teloxide::net::Download;
use teloxide::requests::Requester;
use tracing::{debug, info};

/// Resolve and download any media referenced in the MsgContext.
///
/// If `media_url` starts with `tg://file/`, extracts the file_id,
/// calls `bot.get_file()`, downloads the bytes, saves to disk,
/// and updates `media_path`, `media_file_name`, and `media_mime_type`.
///
/// No-op if there's no media or media_url doesn't match the tg:// scheme.
pub async fn resolve_and_download(
    bot: &teloxide::Bot,
    base_dir: &Path,
    account_id: &str,
    chat_id: &str,
    ctx: &mut claw_channels::msg_context::MsgContext,
) -> Result<(), FileResolverError> {
    let media_url = match &ctx.media_url {
        Some(url) if url.starts_with("tg://file/") => url.clone(),
        _ => return Ok(()), // No media or not a tg:// URL — no-op.
    };

    let file_id = media_url["tg://file/".len()..].to_string();
    if file_id.is_empty() {
        return Ok(());
    }

    debug!(%file_id, "resolving telegram file");

    // 1. Get file metadata from Telegram.
    let tg_file = bot
        .get_file(file_id.clone().into())
        .await
        .map_err(|e| FileResolverError::TelegramApi(format!("get_file failed: {e}")))?;

    let tg_file_path = tg_file.path.clone();

    // 2. Determine filename and mime type.
    let (file_name, mime_type) = infer_metadata(ctx, &tg_file_path);

    // 3. Create download directory: {base_dir}/{account_id}/{chat_id}/
    let dir = base_dir.join(account_id).join(chat_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| FileResolverError::Io(format!("failed to create dir {}: {e}", dir.display())))?;

    // 4. Download file bytes.
    let dest_path = dir.join(&file_name);
    let mut file = tokio::fs::File::create(&dest_path)
        .await
        .map_err(|e| FileResolverError::Io(format!("failed to create file {}: {e}", dest_path.display())))?;

    bot.download_file(&tg_file_path, &mut file)
        .await
        .map_err(|e| FileResolverError::Download(format!("download failed: {e}")))?;

    info!(
        file_id,
        path = %dest_path.display(),
        file_name = %file_name,
        mime_type = %mime_type,
        "telegram file downloaded"
    );

    // 5. Update MsgContext with resolved file info.
    ctx.media_path = Some(dest_path.to_string_lossy().into_owned());
    if ctx.media_file_name.is_none() {
        ctx.media_file_name = Some(file_name);
    }
    if ctx.media_mime_type.is_none() {
        ctx.media_mime_type = Some(mime_type);
    }

    Ok(())
}

/// Return the default base directory for file storage.
///
/// Uses `file_storage_dir` from config if set, otherwise falls back
/// to `{system_temp_dir}/claw-files/`.
pub fn default_file_storage_dir(config_dir: Option<&str>) -> PathBuf {
    match config_dir {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::temp_dir().join("claw-files"),
    }
}

/// Infer filename and MIME type from media metadata.
///
/// Uses Telegram-provided metadata when available (Document, Audio),
/// falls back to sensible defaults based on media_type for Photo, Voice, etc.
fn infer_metadata(ctx: &claw_channels::msg_context::MsgContext, tg_file_path: &str) -> (String, String) {
    // If normalize already set a filename from Telegram metadata, use it.
    if let (Some(name), Some(mime)) = (&ctx.media_file_name, &ctx.media_mime_type) {
        return (name.clone(), mime.clone());
    }

    let media_type = ctx.media_type.as_deref().unwrap_or("document");

    // Extract extension from Telegram's file_path (e.g. "photos/file_42.jpg").
    let tg_ext = Path::new(tg_file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Extract the file_id stem for generating names.
    let file_id_stem = ctx
        .media_url
        .as_deref()
        .and_then(|u| u.strip_prefix("tg://file/"))
        .unwrap_or("unknown");
    // Use only the last 12 chars to keep names short.
    let short_id = if file_id_stem.len() > 12 {
        &file_id_stem[file_id_stem.len() - 12..]
    } else {
        file_id_stem
    };

    // If we have a filename from Telegram but no mime, infer mime from filename.
    if let Some(name) = &ctx.media_file_name {
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let mime = ctx
            .media_mime_type
            .clone()
            .unwrap_or_else(|| mime_from_ext(ext));
        return (name.clone(), mime);
    }

    match media_type {
        "photo" => {
            let ext = if tg_ext.is_empty() { "jpg" } else { tg_ext };
            (format!("photo_{short_id}.{ext}"), "image/jpeg".into())
        }
        "document" => {
            let ext = if tg_ext.is_empty() { "bin" } else { tg_ext };
            let mime = mime_from_ext(ext);
            (format!("doc_{short_id}.{ext}"), mime)
        }
        "audio" => {
            let ext = if tg_ext.is_empty() { "mp3" } else { tg_ext };
            let mime = mime_from_ext(ext);
            (format!("audio_{short_id}.{ext}"), mime)
        }
        "video" => {
            let ext = if tg_ext.is_empty() { "mp4" } else { tg_ext };
            (format!("video_{short_id}.{ext}"), "video/mp4".into())
        }
        "voice" => (format!("voice_{short_id}.ogg"), "audio/ogg".into()),
        "animation" => {
            let ext = if tg_ext.is_empty() { "mp4" } else { tg_ext };
            (format!("anim_{short_id}.{ext}"), "video/mp4".into())
        }
        "sticker" => {
            let ext = if tg_ext.is_empty() { "webp" } else { tg_ext };
            let mime = mime_from_ext(ext);
            (format!("sticker_{short_id}.{ext}"), mime)
        }
        _ => {
            let ext = if tg_ext.is_empty() { "bin" } else { tg_ext };
            let mime = mime_from_ext(ext);
            (format!("file_{short_id}.{ext}"), mime)
        }
    }
}

/// Map common file extensions to MIME types.
pub(crate) fn mime_from_ext(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "zip" => "application/zip",
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        "webm" => "video/webm",
        "tgs" => "application/x-tgsticker",
        _ => "application/octet-stream",
    }
    .into()
}

/// Errors from the file resolver.
#[derive(Debug, thiserror::Error)]
pub enum FileResolverError {
    #[error("Telegram API error: {0}")]
    TelegramApi(String),

    #[error("File download error: {0}")]
    Download(String),

    #[error("I/O error: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_from_common_extensions() {
        assert_eq!(mime_from_ext("jpg"), "image/jpeg");
        assert_eq!(mime_from_ext("jpeg"), "image/jpeg");
        assert_eq!(mime_from_ext("png"), "image/png");
        assert_eq!(mime_from_ext("pdf"), "application/pdf");
        assert_eq!(mime_from_ext("csv"), "text/csv");
        assert_eq!(mime_from_ext("xlsx"), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        assert_eq!(mime_from_ext("mp4"), "video/mp4");
        assert_eq!(mime_from_ext("ogg"), "audio/ogg");
        assert_eq!(mime_from_ext("unknown"), "application/octet-stream");
    }

    #[test]
    fn mime_case_insensitive() {
        assert_eq!(mime_from_ext("JPG"), "image/jpeg");
        assert_eq!(mime_from_ext("PDF"), "application/pdf");
        assert_eq!(mime_from_ext("Mp4"), "video/mp4");
    }

    #[test]
    fn default_dir_uses_config_when_set() {
        let dir = default_file_storage_dir(Some("/custom/path"));
        assert_eq!(dir, PathBuf::from("/custom/path"));
    }

    #[test]
    fn default_dir_falls_back_to_temp() {
        let dir = default_file_storage_dir(None);
        assert!(dir.to_string_lossy().contains("claw-files"));
    }

    #[test]
    fn default_dir_ignores_empty_string() {
        let dir = default_file_storage_dir(Some(""));
        assert!(dir.to_string_lossy().contains("claw-files"));
    }

    #[test]
    fn infer_metadata_uses_existing_name_and_mime() {
        let mut ctx = claw_channels::msg_context::MsgContext::default();
        ctx.media_file_name = Some("report.pdf".into());
        ctx.media_mime_type = Some("application/pdf".into());
        ctx.media_type = Some("document".into());
        ctx.media_url = Some("tg://file/abc123".into());

        let (name, mime) = infer_metadata(&ctx, "documents/file_42.pdf");
        assert_eq!(name, "report.pdf");
        assert_eq!(mime, "application/pdf");
    }

    #[test]
    fn infer_metadata_photo_defaults() {
        let mut ctx = claw_channels::msg_context::MsgContext::default();
        ctx.media_type = Some("photo".into());
        ctx.media_url = Some("tg://file/AbCdEfGhIjKl".into());

        let (name, mime) = infer_metadata(&ctx, "photos/file_99.jpg");
        assert_eq!(name, "photo_AbCdEfGhIjKl.jpg");
        assert_eq!(mime, "image/jpeg");
    }

    #[test]
    fn infer_metadata_voice_defaults() {
        let mut ctx = claw_channels::msg_context::MsgContext::default();
        ctx.media_type = Some("voice".into());
        ctx.media_url = Some("tg://file/voice12ab".into());

        let (name, mime) = infer_metadata(&ctx, "voice/AwACAgIAAxkB.oga");
        assert_eq!(name, "voice_voice12ab.ogg");
        assert_eq!(mime, "audio/ogg");
    }

    #[test]
    fn infer_metadata_uses_tg_extension() {
        let mut ctx = claw_channels::msg_context::MsgContext::default();
        ctx.media_type = Some("document".into());
        ctx.media_url = Some("tg://file/fileid12".into());

        let (name, mime) = infer_metadata(&ctx, "documents/file_42.xlsx");
        assert_eq!(name, "doc_fileid12.xlsx");
        assert_eq!(mime, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
    }

    #[test]
    fn infer_metadata_truncates_long_ids() {
        let mut ctx = claw_channels::msg_context::MsgContext::default();
        ctx.media_type = Some("photo".into());
        ctx.media_url = Some("tg://file/BQACAgQAAxkBAAIDF2abcdefghij".into());

        let (name, _) = infer_metadata(&ctx, "photos/file.jpg");
        // Last 12 chars of the file_id
        assert!(name.starts_with("photo_"));
        assert!(name.len() < 30); // Reasonable length
    }
}
