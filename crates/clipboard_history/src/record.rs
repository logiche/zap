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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_preview_短文本原样返回() {
        assert_eq!(make_preview("hello"), "hello");
    }

    #[test]
    fn make_preview_空字符串返回空() {
        assert_eq!(make_preview(""), "");
    }

    #[test]
    fn make_preview_多行合并为单行() {
        assert_eq!(make_preview("line1\nline2\nline3"), "line1 line2 line3");
    }

    #[test]
    fn make_preview_超长文本截断100字符() {
        let long = "a".repeat(150);
        let result = make_preview(&long);
        assert_eq!(result.chars().count(), 101); // 100 + …
        assert!(result.ends_with('…'));
        assert_eq!(&result[..100], &"a".repeat(100));
    }

    #[test]
    fn make_preview_刚好100字符不截断() {
        let exact = "a".repeat(100);
        let result = make_preview(&exact);
        assert_eq!(result, exact);
        assert!(!result.contains('…'));
    }

    #[test]
    fn make_preview_前后空白被去除() {
        assert_eq!(make_preview("  hello world  "), "hello world");
    }

    #[test]
    fn make_preview_包含空行的多行文本() {
        assert_eq!(make_preview("line1\n\nline3"), "line1  line3");
    }
}
