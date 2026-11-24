use crate::{
    error::{AppError, AppResult},
    security::sanitize_original_filename,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone)]
pub struct ValidatedUpload {
    pub original_filename: String,
    pub extension: String,
    pub declared_mime: String,
    pub detected_mime: String,
    pub sha256_hash: String,
    pub file_size_bytes: i64,
    pub stored_blob_key: String,
}

#[derive(Clone)]
pub struct DocumentStorage {
    root_dir: PathBuf,
    max_upload_bytes: usize,
}

impl DocumentStorage {
    pub fn new(root_dir: PathBuf, max_upload_bytes: usize) -> Self {
        Self {
            root_dir,
            max_upload_bytes,
        }
    }

    pub async fn ensure_initialized(&self) -> AppResult<()> {
        let blobs = self.root_dir.join("blobs");
        fs::create_dir_all(&blobs)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create storage directory: {}", e)))?;
        Ok(())
    }

    pub fn validate_and_detect(
        &self,
        raw_filename: &str,
        declared_mime: &str,
        bytes: &[u8],
    ) -> AppResult<(String, String, String)> {
        let filename = sanitize_original_filename(raw_filename)?;
        if bytes.is_empty() {
            return Err(AppError::BadRequest("Uploaded file is empty.".into()));
        }
        if bytes.len() > self.max_upload_bytes {
            return Err(AppError::BadRequest(format!(
                "File size exceeds maximum limit of {} bytes.",
                self.max_upload_bytes
            )));
        }

        let ext = Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .ok_or_else(|| AppError::BadRequest("File must have a valid extension.".into()))?;

        let detected_mime = detect_mime_from_magic(bytes, &ext)?;

        // Verify extension matches detected content type
        let valid_pair = match ext.as_str() {
            "pdf" => detected_mime == "application/pdf",
            "png" => detected_mime == "image/png",
            "jpg" | "jpeg" => detected_mime == "image/jpeg",
            "tif" | "tiff" => detected_mime == "image/tiff",
            "docx" => {
                detected_mime
                    == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            "xlsx" => {
                detected_mime
                    == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            "csv" => detected_mime == "text/csv",
            _ => false,
        };

        if !valid_pair {
            return Err(AppError::BadRequest(format!(
                "Unsupported or mismatched file format: extension '.{}' does not match file signature ({}).",
                ext, detected_mime
            )));
        }

        // Cross-check declared MIME if provided (allow application/octet-stream from generic clients)
        let norm_declared = declared_mime.trim().to_ascii_lowercase();
        if !norm_declared.is_empty()
            && norm_declared != "application/octet-stream"
            && norm_declared != detected_mime
            && !(ext == "csv" && (norm_declared == "text/plain" || norm_declared == "application/vnd.ms-excel"))
        {
            return Err(AppError::BadRequest(format!(
                "Declared MIME type '{}' conflicts with detected file type '{}'.",
                declared_mime, detected_mime
            )));
        }

        Ok((filename, ext, detected_mime.to_string()))
    }

    pub async fn store_immutable(
        &self,
        raw_filename: &str,
        declared_mime: &str,
        bytes: &[u8],
    ) -> AppResult<ValidatedUpload> {
        let (original_filename, extension, detected_mime) =
            self.validate_and_detect(raw_filename, declared_mime, bytes)?;

        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let sha256_hash = hex::encode(hasher.finalize());

        let blob_path = self.blob_path_for_key(&sha256_hash)?;
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Internal(format!("Storage mkdir failed: {}", e)))?;
        }

        // Preserve original uploaded bytes immutably; never overwrite an existing content-addressed blob
        if !blob_path.exists() {
            let tmp_path = blob_path.with_extension("tmp");
            fs::write(&tmp_path, bytes)
                .await
                .map_err(|e| AppError::Internal(format!("Storage write failed: {}", e)))?;
            fs::rename(&tmp_path, &blob_path)
                .await
                .map_err(|e| AppError::Internal(format!("Storage commit failed: {}", e)))?;
        }

        Ok(ValidatedUpload {
            original_filename,
            extension,
            declared_mime: if declared_mime.is_empty() {
                detected_mime.clone()
            } else {
                declared_mime.to_string()
            },
            detected_mime,
            sha256_hash: sha256_hash.clone(),
            file_size_bytes: bytes.len() as i64,
            stored_blob_key: sha256_hash,
        })
    }

    pub async fn read_blob(&self, stored_blob_key: &str) -> AppResult<Vec<u8>> {
        let path = self.blob_path_for_key(stored_blob_key)?;
        fs::read(&path)
            .await
            .map_err(|_| AppError::NotFound("Stored document content not found.".into()))
    }

    pub fn blob_path_for_key(&self, stored_blob_key: &str) -> AppResult<PathBuf> {
        // Strict 64-char lowercase hex check prevents any path traversal
        if stored_blob_key.len() != 64
            || !stored_blob_key
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(AppError::BadRequest("Invalid document storage key.".into()));
        }
        let shard = &stored_blob_key[0..2];
        Ok(self.root_dir.join("blobs").join(shard).join(stored_blob_key))
    }
}

fn detect_mime_from_magic(bytes: &[u8], ext: &str) -> AppResult<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        return Ok("application/pdf");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Ok("image/png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Ok("image/jpeg");
    }
    if bytes.len() >= 4
        && ((bytes[0] == b'I' && bytes[1] == b'I' && bytes[2] == 42 && bytes[3] == 0)
            || (bytes[0] == b'M' && bytes[1] == b'M' && bytes[2] == 0 && bytes[3] == 42))
    {
        return Ok("image/tiff");
    }
    // OOXML (DOCX/XLSX) are ZIP containers starting with PK\x03\x04
    if bytes.len() >= 4 && bytes[0..4] == [0x50, 0x4B, 0x03, 0x04] {
        // Inspect zip entry names in bytes for word/ vs xl/ markers
        if ext == "docx" && bytes.windows(5).any(|w| w == b"word/") {
            return Ok("application/vnd.openxmlformats-officedocument.wordprocessingml.document");
        }
        if ext == "xlsx" && bytes.windows(3).any(|w| w == b"xl/") {
            return Ok("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        }
        return Err(AppError::BadRequest(
            "ZIP archive does not match expected DOCX or XLSX internal structure.".into(),
        ));
    }
    // CSV validation: must be valid UTF-8, no null bytes, no binary control chars
    if ext == "csv" {
        if let Ok(text) = std::str::from_utf8(bytes) {
            let has_binary = text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t');
            if !has_binary && (text.contains(',') || text.contains(';') || text.contains('\t')) {
                return Ok("text/csv");
            }
        }
    }
    Err(AppError::BadRequest(
        "Unsupported or corrupted file content; file signature validation failed.".into(),
    ))
}
