//! 剪贴板页面 UI 组件
//!
//! 渲染搜索框、记录列表、清空按钮。
//! 提供数据结构和事件定义，由胶水层完成实际渲染。
//!
//! author logic
//! date 2026-05-31

use chrono::{DateTime, Utc};

/// 剪贴板页面的 Action 事件
#[derive(Clone, Debug)]
pub enum ClipboardPageAction {
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

/// 格式化时间为显示用字符串
pub fn format_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);

    if diff.num_seconds() < 60 {
        "刚刚".to_string()
    } else if diff.num_minutes() < 60 {
        let mins = diff.num_minutes();
        format!("{mins}分钟前")
    } else if diff.num_hours() < 24 {
        let hours = diff.num_hours();
        format!("{hours}小时前")
    } else if diff.num_days() < 7 {
        let days = diff.num_days();
        format!("{days}天前")
    } else {
        dt.format("%m-%d %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn format_time_刚刚() {
        let now = Utc::now();
        assert_eq!(format_time(&now), "刚刚");
    }

    #[test]
    fn format_time_59秒前显示刚刚() {
        let dt = Utc::now() - Duration::seconds(59);
        assert_eq!(format_time(&dt), "刚刚");
    }

    #[test]
    fn format_time_分钟前() {
        let dt = Utc::now() - Duration::minutes(5);
        assert_eq!(format_time(&dt), "5分钟前");
    }

    #[test]
    fn format_time_小时前() {
        let dt = Utc::now() - Duration::hours(3);
        assert_eq!(format_time(&dt), "3小时前");
    }

    #[test]
    fn format_time_天前() {
        let dt = Utc::now() - Duration::days(2);
        assert_eq!(format_time(&dt), "2天前");
    }

    #[test]
    fn format_time_超过一周显示日期() {
        let dt = Utc::now() - Duration::days(10);
        let result = format_time(&dt);
        // 格式为 "MM-DD HH:MM"
        assert!(result.contains('-'));
        assert!(result.contains(':'));
    }
}
