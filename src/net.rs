//! Small blocking HTTP helper built on `ureq`.
//!
//! Everything here is synchronous: callers run it on a worker thread and talk
//! back to the UI over a channel. Downloads use a temp file + rename so an
//! interrupted transfer never leaves a half written cache entry behind.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/126.0 Safari/537.36 EchoPort/0.1";

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("network error: {0}")]
    Transport(String),
    #[error("HTTP status {0}")]
    Status(u16),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode error: {0}")]
    Decode(String),
}

impl NetError {
    pub fn is_status(&self, code: u16) -> bool {
        matches!(self, Self::Status(c) if *c == code)
    }
}

pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub fn new() -> Self {
        // Keep timeouts snappy: the UI shows progress and can retry.
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(25)))
            .timeout_connect(Some(Duration::from_secs(8)))
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }

    fn request(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<ureq::http::Response<ureq::Body>, NetError> {
        let mut request = self.agent.get(url);
        for (key, value) in headers {
            request = request.header(*key, *value);
        }
        request
            .call()
            .map_err(|err| NetError::Transport(err.to_string()))
    }

    fn ensure_success(response: &ureq::http::Response<ureq::Body>) -> Result<(), NetError> {
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(NetError::Status(status))
        }
    }

    pub fn get_text(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, NetError> {
        let mut response = self.request(url, headers)?;
        Self::ensure_success(&response)?;
        let mut body = String::new();
        response
            .body_mut()
            .as_reader()
            .read_to_string(&mut body)
            .map_err(|err| NetError::Transport(err.to_string()))?;
        Ok(body)
    }

    pub fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<T, NetError> {
        let text = self.get_text(url, headers)?;
        serde_json::from_str(&text).map_err(|err| NetError::Decode(err.to_string()))
    }

    pub fn get_bytes(&self, url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>, NetError> {
        let mut response = self.request(url, headers)?;
        Self::ensure_success(&response)?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut bytes)
            .map_err(|err| NetError::Transport(err.to_string()))?;
        Ok(bytes)
    }

    /// Download a URL into `dest`, reporting progress in bytes.
    pub fn download<F: FnMut(u64, Option<u64>)>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        dest: &Path,
        mut on_progress: F,
    ) -> Result<(), NetError> {
        let mut response = self.request(url, headers)?;
        Self::ensure_success(&response)?;
        let total = response.body().content_length();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = dest.with_extension("part");
        let mut file = std::fs::File::create(&tmp)?;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut written: u64 = 0;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|err| NetError::Transport(err.to_string()))?;
            if read == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buffer[..read])?;
            written += read as u64;
            on_progress(written, total);
        }
        drop(file);
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

/// Percent helper shared by provider implementations.
pub fn percent(written: u64, total: Option<u64>) -> Option<f32> {
    total
        .filter(|t| *t > 0)
        .map(|total| (written as f32 / total as f32).clamp(0.0, 1.0))
}
