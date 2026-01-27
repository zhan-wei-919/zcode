//! 剪贴板服务
//!
//! 封装系统剪贴板操作，支持复制/粘贴/剪切
//! TODO: 大文本粘贴优化（>10MB 时考虑分块处理或警告）

use crate::core::Service;
use std::path::Path;

const PASTE_MAX_SIZE: usize = 10 * 1024 * 1024; // 10MB

#[derive(Debug, Clone, Copy)]
enum ClipboardProvider {
    #[cfg(target_os = "macos")]
    MacOsPasteboard,
    None,
}

pub struct ClipboardService {
    provider: ClipboardProvider,
}

#[derive(Debug)]
pub enum ClipboardError {
    NotAvailable,
    GetFailed(String),
    SetFailed(String),
    TooLarge(usize),
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardError::NotAvailable => write!(f, "剪贴板不可用"),
            ClipboardError::GetFailed(e) => write!(f, "读取剪贴板失败: {}", e),
            ClipboardError::SetFailed(e) => write!(f, "写入剪贴板失败: {}", e),
            ClipboardError::TooLarge(size) => {
                write!(f, "文本过大 ({} MB)，超过 10MB 限制", size / 1024 / 1024)
            }
        }
    }
}

impl ClipboardService {
    pub fn new() -> Self {
        let provider = {
            #[cfg(target_os = "macos")]
            {
                if command_exists("pbcopy") && command_exists("pbpaste") {
                    ClipboardProvider::MacOsPasteboard
                } else {
                    ClipboardProvider::None
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                ClipboardProvider::None
            }
        };

        Self { provider }
    }

    pub fn is_available(&self) -> bool {
        !matches!(self.provider, ClipboardProvider::None)
    }

    pub fn get_text(&mut self) -> Result<String, ClipboardError> {
        match self.provider {
            #[cfg(target_os = "macos")]
            ClipboardProvider::MacOsPasteboard => macos::pbpaste_text(),
            ClipboardProvider::None => Err(ClipboardError::NotAvailable),
        }
    }

    pub fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        match self.provider {
            #[cfg(target_os = "macos")]
            ClipboardProvider::MacOsPasteboard => macos::pbcopy_text(text),
            ClipboardProvider::None => Err(ClipboardError::NotAvailable),
        }
    }
}

fn command_exists(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }

    // If a path is provided, just check the file exists.
    if name.contains('/') || name.contains('\\') {
        return Path::new(name).is_file();
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{ClipboardError, PASTE_MAX_SIZE};
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    pub(super) fn pbpaste_text() -> Result<String, ClipboardError> {
        let mut child = Command::new("pbpaste")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ClipboardError::GetFailed(e.to_string()))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| ClipboardError::GetFailed("pbpaste missing stdout".to_string()))?;

        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = stdout
                .read(&mut chunk)
                .map_err(|e| ClipboardError::GetFailed(e.to_string()))?;
            if n == 0 {
                break;
            }
            if buf.len().saturating_add(n) > PASTE_MAX_SIZE {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ClipboardError::TooLarge(buf.len().saturating_add(n)));
            }
            buf.extend_from_slice(&chunk[..n]);
        }

        let status = child
            .wait()
            .map_err(|e| ClipboardError::GetFailed(e.to_string()))?;
        if !status.success() {
            return Err(ClipboardError::GetFailed(format!(
                "pbpaste failed: {}",
                status
            )));
        }

        String::from_utf8(buf).map_err(|e| ClipboardError::GetFailed(e.to_string()))
    }

    pub(super) fn pbcopy_text(text: &str) -> Result<(), ClipboardError> {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ClipboardError::SetFailed(e.to_string()))?;

        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| ClipboardError::SetFailed("pbcopy missing stdin".to_string()))?;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| ClipboardError::SetFailed(e.to_string()))?;
        }

        let status = child
            .wait()
            .map_err(|e| ClipboardError::SetFailed(e.to_string()))?;
        if !status.success() {
            return Err(ClipboardError::SetFailed(format!(
                "pbcopy failed: {}",
                status
            )));
        }

        Ok(())
    }
}

impl Default for ClipboardService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service for ClipboardService {
    fn name(&self) -> &'static str {
        "ClipboardService"
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/services/adapters/clipboard.rs"]
mod tests;
