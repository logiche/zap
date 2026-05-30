//! 剪贴板内容变化监听
//!
//! 通过轮询 arboard 检测剪贴板变化。
//!
//! author logic
//! date 2026-05-31

use arboard::Clipboard;

/// 剪贴板监听器
pub struct ClipboardWatcher {
    last_content: Option<String>,
}

impl ClipboardWatcher {
    /// 创建新的监听器
    pub fn new() -> Self {
        Self {
            last_content: None,
        }
    }

    /// 检查剪贴板是否有新的文本内容
    ///
    /// 返回 Some(content) 表示有新内容，None 表示无变化或非文本
    pub fn poll(&mut self) -> Option<String> {
        let mut clipboard = match Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return None,
        };

        let text = match clipboard.get_text() {
            Ok(text) => text,
            Err(_) => return None,
        };

        if self.last_content.as_deref() == Some(text.as_str()) {
            return None;
        }

        self.last_content = Some(text.clone());
        Some(text)
    }
}

impl Default for ClipboardWatcher {
    fn default() -> Self {
        Self::new()
    }
}
