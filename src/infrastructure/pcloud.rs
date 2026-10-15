use crate::domain::backup::{BackupError, BoxFuture, PCloudClientPort, PCloudRegion, PCloudRemoteFile};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone)]
pub struct HttpPCloudClient {
    client: reqwest::Client,
}

impl HttpPCloudClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(600)) // 10 minutes timeout for large vaults
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for HttpPCloudClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct PCloudUploadResponse {
    result: i32,
    error: Option<String>,
}

struct FileStreamBody {
    stream: std::sync::Mutex<tokio_util::io::ReaderStream<tokio::fs::File>>,
    len: u64,
}

impl FileStreamBody {
    fn new(file: tokio::fs::File, len: u64) -> Self {
        Self {
            stream: std::sync::Mutex::new(tokio_util::io::ReaderStream::new(file)),
            len,
        }
    }
}

impl http_body::Body for FileStreamBody {
    type Data = bytes::Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        use futures_util::Stream;
        let mut guard = self.stream.lock().unwrap();
        match std::pin::Pin::new(&mut *guard).poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => {
                std::task::Poll::Ready(Some(Ok(http_body::Frame::data(bytes))))
            }
            std::task::Poll::Ready(Some(Err(e))) => std::task::Poll::Ready(Some(Err(e))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::with_exact(self.len)
    }
}

impl PCloudClientPort for HttpPCloudClient {
    fn upload(
        &self,
        region: &PCloudRegion,
        token: &str,
        folder: &str,
        filename: &str,
        file_path: &std::path::Path,
    ) -> BoxFuture<'_, Result<(), BackupError>> {
        let region = region.clone();
        let token = token.to_string();
        let folder = folder.trim().to_string();
        let filename = filename.to_string();
        let path = file_path.to_path_buf();

        Box::pin(async move {
            if token.is_empty() {
                return Err(BackupError::PCloudUploadFailed(
                    "pCloud access token is not configured".to_string(),
                ));
            }

            let file = tokio::fs::File::open(&path)
                .await
                .map_err(|e| BackupError::IoError(format!("Failed to open file for upload: {e}")))?;
            let file_len = file
                .metadata()
                .await
                .map_err(|e| BackupError::IoError(format!("Failed to read file metadata for upload: {e}")))?
                .len();

            let folder_param = normalize_folder_param(&folder);

            let url = format!(
                "https://{}/uploadfile?{}&access_token={}&nopartial=1",
                region.api_host(),
                folder_param,
                token
            );

            let stream_body = FileStreamBody::new(file, file_len);
            let body = reqwest::Body::wrap(stream_body);
            let part = reqwest::multipart::Part::stream_with_length(body, file_len)
                .file_name(filename.clone());
            let form = reqwest::multipart::Form::new().part("file", part);

            let res = self
                .client
                .post(&url)
                .multipart(form)
                .send()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("HTTP request failed: {e}")))?;

            if !res.status().is_success() {
                return Err(BackupError::PCloudUploadFailed(format!(
                    "pCloud server returned HTTP status {}",
                    res.status()
                )));
            }

            let body_text = res
                .text()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("Failed to read response: {e}")))?;

            match serde_json::from_str::<PCloudUploadResponse>(&body_text) {
                Ok(resp) => {
                    if resp.result != 0 {
                        let err_msg = resp.error.unwrap_or_else(|| format!("pCloud error code {}", resp.result));
                        return Err(BackupError::PCloudUploadFailed(err_msg));
                    }
                    Ok(())
                }
                Err(_) => {
                    // If response is not standard json error or success, check if contains success indicators
                    if body_text.contains("\"result\":0") || body_text.contains("\"result\": 0") {
                        Ok(())
                    } else {
                        Err(BackupError::PCloudUploadFailed(format!(
                            "Invalid pCloud API response: {body_text}"
                        )))
                    }
                }
            }
        })
    }

    fn list_folder(
        &self,
        region: &PCloudRegion,
        token: &str,
        folder: &str,
    ) -> BoxFuture<'_, Result<Vec<PCloudRemoteFile>, BackupError>> {
        let region = region.clone();
        let token = token.to_string();
        let folder = folder.trim().to_string();

        Box::pin(async move {
            if token.is_empty() {
                return Err(BackupError::PCloudUploadFailed(
                    "pCloud access token is not configured".to_string(),
                ));
            }

            let folder_param = normalize_folder_param(&folder);

            let url = format!(
                "https://{}/listfolder?{}&access_token={}",
                region.api_host(),
                folder_param,
                token
            );

            let res = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("HTTP listfolder request failed: {e}")))?;

            if !res.status().is_success() {
                return Err(BackupError::PCloudUploadFailed(format!(
                    "pCloud server returned HTTP status {}",
                    res.status()
                )));
            }

            let body_text = res
                .text()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("Failed to read response: {e}")))?;

            #[derive(Deserialize)]
            struct PCloudListFolderResponse {
                result: i32,
                error: Option<String>,
                metadata: Option<PCloudFolderMetadata>,
            }

            #[derive(Deserialize)]
            struct PCloudFolderMetadata {
                contents: Option<Vec<PCloudItemMetadata>>,
            }

            #[derive(Deserialize)]
            struct PCloudItemMetadata {
                fileid: Option<u64>,
                name: String,
                size: Option<u64>,
                isfolder: bool,
                modified: Option<String>,
            }

            let resp: PCloudListFolderResponse = serde_json::from_str(&body_text)
                .map_err(|e| BackupError::PCloudUploadFailed(format!("Failed to parse listfolder JSON: {e}")))?;

            if resp.result != 0 {
                let err_msg = resp.error.unwrap_or_else(|| format!("pCloud error code {}", resp.result));
                return Err(BackupError::PCloudUploadFailed(err_msg));
            }

            let mut files = Vec::new();
            if let Some(contents) = resp.metadata.and_then(|m| m.contents) {
                for item in contents {
                    if !item.isfolder && let Some(fileid) = item.fileid {
                        files.push(PCloudRemoteFile {
                            fileid,
                            name: item.name,
                            size_bytes: item.size.unwrap_or(0),
                            modified: item.modified.unwrap_or_default(),
                        });
                    }
                }
            }

            files.sort_by(|a, b| b.name.cmp(&a.name));
            Ok(files)
        })
    }

    fn download_file(
        &self,
        region: &PCloudRegion,
        token: &str,
        fileid: u64,
        dest_path: &std::path::Path,
    ) -> BoxFuture<'_, Result<(), BackupError>> {
        let region = region.clone();
        let token = token.to_string();
        let dest_path = dest_path.to_path_buf();

        Box::pin(async move {
            if token.is_empty() {
                return Err(BackupError::PCloudUploadFailed(
                    "pCloud access token is not configured".to_string(),
                ));
            }

            let url = format!(
                "https://{}/getfilelink?fileid={}&access_token={}",
                region.api_host(),
                fileid,
                token
            );

            let res = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("HTTP getfilelink request failed: {e}")))?;

            if !res.status().is_success() {
                return Err(BackupError::PCloudUploadFailed(format!(
                    "pCloud server returned HTTP status {}",
                    res.status()
                )));
            }

            #[derive(Deserialize)]
            struct PCloudGetFileLinkResponse {
                result: i32,
                error: Option<String>,
                hosts: Option<Vec<String>>,
                path: Option<String>,
            }

            let body_text = res
                .text()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("Failed to read response: {e}")))?;

            let link_resp: PCloudGetFileLinkResponse = serde_json::from_str(&body_text)
                .map_err(|e| BackupError::PCloudUploadFailed(format!("Failed to parse getfilelink JSON: {e}")))?;

            if link_resp.result != 0 {
                let err_msg = link_resp.error.unwrap_or_else(|| format!("pCloud error code {}", link_resp.result));
                return Err(BackupError::PCloudUploadFailed(err_msg));
            }

            let host = link_resp
                .hosts
                .and_then(|h| h.into_iter().next())
                .ok_or_else(|| BackupError::PCloudUploadFailed("No host returned for file download".to_string()))?;
            let path = link_resp
                .path
                .ok_or_else(|| BackupError::PCloudUploadFailed("No path returned for file download".to_string()))?;

            let download_url = format!("https://{}{}", host, path);
            let mut download_res = self
                .client
                .get(&download_url)
                .send()
                .await
                .map_err(|e| BackupError::PCloudUploadFailed(format!("Failed to initiate file download: {e}")))?;

            if !download_res.status().is_success() {
                return Err(BackupError::PCloudUploadFailed(format!(
                    "Download endpoint returned HTTP status {}",
                    download_res.status()
                )));
            }

            let mut out_file = tokio::fs::File::create(&dest_path)
                .await
                .map_err(|e| BackupError::IoError(format!("Failed to create destination file: {e}")))?;

            while let Some(chunk) = download_res
                .chunk()
                .await
                .map_err(|e| BackupError::IoError(format!("Failed reading download chunk: {e}")))?
            {
                use tokio::io::AsyncWriteExt;
                out_file
                    .write_all(&chunk)
                    .await
                    .map_err(|e| BackupError::IoError(format!("Failed writing download chunk: {e}")))?;
            }

            use tokio::io::AsyncWriteExt;
            out_file
                .flush()
                .await
                .map_err(|e| BackupError::IoError(format!("Failed flushing downloaded file: {e}")))?;

            Ok(())
        })
    }
}

pub(crate) fn normalize_folder_param(folder: &str) -> String {
    let trimmed = folder.trim();
    if trimmed.is_empty() || trimmed == "0" {
        "folderid=0".to_string()
    } else if trimmed.chars().all(|c| c.is_ascii_digit()) {
        format!("folderid={}", trimmed)
    } else {
        let mut path = trimmed.trim_end_matches('/').to_string();
        if !path.starts_with('/') {
            path.insert(0, '/');
        }
        format!("path={}", urlencoding_path(&path))
    }
}

pub(crate) fn urlencoding_path(val: &str) -> String {
    let mut out = String::new();
    for b in val.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' || b == b'/' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub(crate) fn urlencoding(val: &str) -> String {
    let mut out = String::new();
    for b in val.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_folder_param() {
        assert_eq!(normalize_folder_param(""), "folderid=0");
        assert_eq!(normalize_folder_param("0"), "folderid=0");
        assert_eq!(normalize_folder_param("123456"), "folderid=123456");
        assert_eq!(normalize_folder_param("/Backups/Vaultwarden"), "path=/Backups/Vaultwarden");
        assert_eq!(normalize_folder_param("/Backups/Vaultwarden/"), "path=/Backups/Vaultwarden");
        assert_eq!(normalize_folder_param("Backups/Vaultwarden"), "path=/Backups/Vaultwarden");
        assert_eq!(normalize_folder_param("Backups/My Folder"), "path=/Backups/My%20Folder");
    }

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding_path("/Vaultwarden/Backups"), "/Vaultwarden/Backups");
        assert_eq!(urlencoding("http://localhost:3000/callback"), "http%3A%2F%2Flocalhost%3A3000%2Fcallback");
        assert_eq!(urlencoding("My Backups"), "My%20Backups");
    }

    #[tokio::test]
    async fn test_empty_token_fails() {
        let client = HttpPCloudClient::new();
        let dummy_path = std::path::Path::new("Cargo.toml");
        let res = client
            .upload(&PCloudRegion::EU, "", "0", "test.enc", dummy_path)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("access token is not configured"));
    }
}
