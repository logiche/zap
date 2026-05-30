//! 剪贴板历史记录数据结构
//!
//! author logic
//! date 2026-05-31

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 一条剪贴板历史记录
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipboardRecord {
    /// 数据库行 ID
    pub id: i64,
    /// 完整文本内容
    pub content: String,
    /// 预览文本（前 100 字符，单行）
    pub preview: String,
    /// 复制时间
    pub created_at: DateTime<Utc>,
}

/// 从完整文本生成预览
pub fn make_preview(content: &str) -> String {
    let single_line: String = content.lines().collect::<Vec<_>>().join(" ");
    let trimmed = single_line.trim();
    if trimmed.len() > 100 {
        format!("{}…", &trimmed[..100])
    } else {
        trimmed.to_string()
    }
}
