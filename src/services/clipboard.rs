//! 剪贴板服务
//!
//! 封装系统剪贴板操作，支持复制/粘贴/剪切
//! TODO: 大文本粘贴优化（>10MB 时考虑分块处理或警告）

use crate::core::Service;
use arboard::Clipboard;

const PASTE_MAX_SIZE: usize = 10 * 1024 * 1024; // 10MB

pub struct ClipboardService {
    clipboard: Option<Clipboard>,
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
        let clipboard = Clipboard::new().ok();
        Self { clipboard }
    }

    pub fn is_available(&self) -> bool {
        self.clipboard.is_some()
    }

    pub fn get_text(&mut self) -> Result<String, ClipboardError> {
        let clipboard = self
            .clipboard
            .as_mut()
            .ok_or(ClipboardError::NotAvailable)?;

        let text = clipboard
            .get_text()
            .map_err(|e| ClipboardError::GetFailed(e.to_string()))?;

        if text.len() > PASTE_MAX_SIZE {
            return Err(ClipboardError::TooLarge(text.len()));
        }

        Ok(text)
    }

    pub fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        let clipboard = self
            .clipboard
            .as_mut()
            .ok_or(ClipboardError::NotAvailable)?;

        clipboard
            .set_text(text.to_string())
            .map_err(|e| ClipboardError::SetFailed(e.to_string()))
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
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_service_creation() {
        let service = ClipboardService::new();
        assert_eq!(service.name(), "ClipboardService");
    }
}
