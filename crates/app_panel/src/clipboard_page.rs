//! 剪贴板页面 UI 组件
//!
//! 渲染搜索框、记录列表、清空按钮。
//! 提供数据结构和事件定义，由胶水层完成实际渲染。
//!
//! author logic
//! date 2026-05-31

use chrono::{DateTime, Utc};
use clipboard_history::ClipboardRecord;

/// 剪贴板页面的 Action 事件
#[derive(Clone, Debug)]
pub enum ClipboardPageAction {
    /// 搜索关键词变更
    SearchQueryChanged(String),
    /// 点击一条记录（复制到剪贴板）
    RecordClicked(i64),
    /// 删除一条记录
    RecordDeleted(i64),
    /// 请求全部清空
    ClearAllRequested,
    /// 确认全部清空
    ClearAllConfirmed,
    /// 取消清空
    ClearAllCancelled,
}

/// 剪贴板页面状态快照（用于渲染）
pub struct ClipboardPageState<'a> {
    /// 当前过滤后的记录列表
    pub records: Vec<&'a ClipboardRecord>,
    /// 当前搜索关键词
    pub search_query: &'a str,
    /// 是否正在显示"确认清空"弹窗
    pub confirm_clear_shown: bool,
}

/// 格式化时间为显示用字符串
pub fn format_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);

    if diff.num_seconds() < 60 {
        "刚刚".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}分钟前", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}小时前", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}天前", diff.num_days())
    } else {
        dt.format("%m-%d %H:%M").to_string()
    }
}
