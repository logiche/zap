//! 应用面板核心 View
//!
//! 管理侧边导航 + 内容区布局的状态。
//! 实际的 View trait 实现由 app/ 中的胶水层完成。
//!
//! author logic
//! date 2026-05-31

use clipboard_history::{ClipboardHistoryModel, ClipboardRecord};
use warpui::Entity;

use crate::clipboard_page::ClipboardPageAction;
use crate::nav::AppPanelSection;

/// 应用面板 View 的外部事件（供胶水层订阅）
#[derive(Clone, Debug)]
pub enum AppPanelViewInnerEvent {
    /// 请求关闭面板
    Close,
    /// 请求显示 toast
    ShowToast { message: String },
}

/// 应用面板核心状态
pub struct AppPanelViewInner {
    /// 当前子页面
    pub current_section: AppPanelSection,
    /// 搜索关键词
    pub search_query: String,
    /// 是否显示清空确认弹窗
    pub confirm_clear_shown: bool,
}

impl Entity for AppPanelViewInner {
    type Event = AppPanelViewInnerEvent;
}

impl AppPanelViewInner {
    /// 创建新的 View 状态
    pub fn new() -> Self {
        Self {
            current_section: AppPanelSection::default(),
            search_query: String::new(),
            confirm_clear_shown: false,
        }
    }

    /// 获取当前过滤后的剪贴板记录
    pub fn filtered_records<'a>(
        &self,
        records: &'a [ClipboardRecord],
    ) -> Vec<&'a ClipboardRecord> {
        if self.search_query.is_empty() {
            records.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            records
                .iter()
                .filter(|r| r.content.to_lowercase().contains(&query))
                .collect()
        }
    }

    /// 处理剪贴板页面 Action
    pub fn handle_clipboard_action(
        &mut self,
        action: &ClipboardPageAction,
        model: &mut ClipboardHistoryModel,
    ) -> Vec<AppPanelViewInnerEvent> {
        let mut events = Vec::new();
        match action {
            ClipboardPageAction::SearchQueryChanged(query) => {
                self.search_query = query.clone();
            }
            ClipboardPageAction::RecordClicked(id) => {
                if let Some(record) = model.records().iter().find(|r| r.id == *id) {
                    let content = record.content.clone();
                    events.push(AppPanelViewInnerEvent::ShowToast {
                        message: format!("已复制: {}", &content[..content.len().min(50)]),
                    });
                }
            }
            ClipboardPageAction::RecordDeleted(id) => {
                let _ = model.delete(*id);
            }
            ClipboardPageAction::ClearAllRequested => {
                self.confirm_clear_shown = true;
            }
            ClipboardPageAction::ClearAllConfirmed => {
                let _ = model.clear_all();
                self.confirm_clear_shown = false;
                events.push(AppPanelViewInnerEvent::ShowToast {
                    message: "已清空全部剪贴板历史".to_string(),
                });
            }
            ClipboardPageAction::ClearAllCancelled => {
                self.confirm_clear_shown = false;
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_history::ClipboardRecord;
    use chrono::Utc;

    fn test_view() -> AppPanelViewInner {
        AppPanelViewInner::new()
    }

    fn test_model() -> ClipboardHistoryModel {
        ClipboardHistoryModel::new_in_memory().expect("failed to create in-memory model")
    }

    fn make_record(id: i64, content: &str) -> ClipboardRecord {
        ClipboardRecord {
            id,
            content: content.to_string(),
            preview: content.chars().take(100).collect(),
            created_at: Utc::now(),
        }
    }

    // --- filtered_records ---

    #[test]
    fn filtered_records_空查询返回全部() {
        let view = test_view();
        let records = vec![make_record(1, "hello"), make_record(2, "world")];

        let filtered = view.filtered_records(&records);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filtered_records_按关键词过滤() {
        let mut view = test_view();
        view.search_query = "rust".to_string();

        let records = vec![
            make_record(1, "Hello Rust"),
            make_record(2, "Python code"),
            make_record(3, "rust programming"),
        ];

        let filtered = view.filtered_records(&records);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, 1);
        assert_eq!(filtered[1].id, 3);
    }

    #[test]
    fn filtered_records_大小写不敏感() {
        let mut view = test_view();
        view.search_query = "HELLO".to_string();

        let records = vec![make_record(1, "hello world")];

        let filtered = view.filtered_records(&records);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filtered_records_无匹配返回空() {
        let mut view = test_view();
        view.search_query = "xyz".to_string();

        let records = vec![make_record(1, "hello")];

        let filtered = view.filtered_records(&records);
        assert!(filtered.is_empty());
    }

    // --- handle_clipboard_action ---

    #[test]
    fn handle_action_search_query_changed() {
        let mut view = test_view();
        let mut model = test_model();

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::SearchQueryChanged("test".to_string()),
            &mut model,
        );

        assert!(events.is_empty());
        assert_eq!(view.search_query, "test");
    }

    #[test]
    fn handle_action_record_clicked_显示toast() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("hello world".to_string()).expect("add failed");
        let record_id = model.records()[0].id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(record_id),
            &mut model,
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AppPanelViewInnerEvent::ShowToast { .. }));
    }

    #[test]
    fn handle_action_record_clicked_不存在的id无事件() {
        let mut view = test_view();
        let mut model = test_model();

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(99999),
            &mut model,
        );

        assert!(events.is_empty());
    }

    #[test]
    fn handle_action_record_deleted() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("to delete".to_string()).expect("add failed");
        let record_id = model.records()[0].id;

        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(record_id),
            &mut model,
        );

        assert!(model.records().is_empty());
    }

    #[test]
    fn handle_action_clear_all_requested_显示确认弹窗() {
        let mut view = test_view();
        let mut model = test_model();

        view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllRequested,
            &mut model,
        );

        assert!(view.confirm_clear_shown);
    }

    #[test]
    fn handle_action_clear_all_confirmed_清空并关闭弹窗() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        view.confirm_clear_shown = true;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllConfirmed,
            &mut model,
        );

        assert!(model.records().is_empty());
        assert!(!view.confirm_clear_shown);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AppPanelViewInnerEvent::ShowToast { .. }));
    }

    #[test]
    fn handle_action_clear_all_cancelled_关闭弹窗() {
        let mut view = test_view();
        let mut model = test_model();

        view.confirm_clear_shown = true;

        view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllCancelled,
            &mut model,
        );

        assert!(!view.confirm_clear_shown);
    }
}
