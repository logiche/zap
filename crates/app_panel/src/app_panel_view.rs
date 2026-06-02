//! 应用面板核心 View
//!
//! 管理侧边导航 + 内容区布局的状态。
//! 实际的 View trait 实现由 app/ 中的胶水层完成。
//!
//! author logic
//! date 2026-05-31

use clipboard_history::ClipboardHistoryModel;
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
            confirm_clear_shown: false,
        }
    }

    /// 切换当前子页面
    pub fn select_section(&mut self, section: AppPanelSection) {
        self.current_section = section;
    }

    /// 处理剪贴板页面 Action
    pub fn handle_clipboard_action(
        &mut self,
        action: &ClipboardPageAction,
        model: &mut ClipboardHistoryModel,
    ) -> Vec<AppPanelViewInnerEvent> {
        let mut events = Vec::new();
        match action {
            ClipboardPageAction::RecordClicked(id) => {
                if let Some(record) = model.records().iter().find(|r| r.id == *id) {
                    let content = record.content.clone();
                    let preview = clipboard_history::truncate_chars(&content, 50);
                    events.push(AppPanelViewInnerEvent::ShowToast {
                        message: format!("已复制: {preview}"),
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

    fn test_view() -> AppPanelViewInner {
        AppPanelViewInner::new()
    }

    fn test_model() -> ClipboardHistoryModel {
        ClipboardHistoryModel::new_in_memory().expect("failed to create in-memory model")
    }

    // --- handle_clipboard_action ---

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

    // --- 初始状态 ---

    #[test]
    fn new_初始状态正确() {
        let view = AppPanelViewInner::new();

        assert_eq!(view.current_section, AppPanelSection::Clipboard);
        assert!(!view.confirm_clear_shown);
    }

    // --- SelectSection 按钮 ---

    #[test]
    fn select_section_切换子页面() {
        let mut view = test_view();

        view.select_section(AppPanelSection::Clipboard);

        assert_eq!(view.current_section, AppPanelSection::Clipboard);
    }

    // --- RecordClicked（点击应用按钮）深入测试 ---

    #[test]
    fn handle_action_record_clicked_toast包含已复制前缀() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("test content".to_string()).expect("add failed");
        let record_id = model.records()[0].id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(record_id),
            &mut model,
        );

        assert_eq!(events.len(), 1);
        if let AppPanelViewInnerEvent::ShowToast { message } = &events[0] {
            assert!(message.starts_with("已复制: "));
            assert!(message.contains("test content"));
        } else {
            panic!("expected ShowToast event");
        }
    }

    #[test]
    fn handle_action_record_clicked_长内容截断显示() {
        let mut view = test_view();
        let mut model = test_model();

        let long_content = "a".repeat(200);
        model.add_record(long_content.clone()).expect("add failed");
        let record_id = model.records()[0].id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(record_id),
            &mut model,
        );

        assert_eq!(events.len(), 1);
        if let AppPanelViewInnerEvent::ShowToast { message } = &events[0] {
            // toast 消息中的内容被截断为最多 50 字符
            let preview_part = message.strip_prefix("已复制: ").expect("should have prefix");
            assert!(preview_part.chars().count() <= 50);
        } else {
            panic!("expected ShowToast event");
        }
    }

    #[test]
    fn handle_action_record_clicked_多条记录点击指定记录() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("first".to_string()).expect("add failed");
        model.add_record("second".to_string()).expect("add failed");
        model.add_record("third".to_string()).expect("add failed");

        // 点击第二条记录（second）
        let second_id = model.records().iter().find(|r| r.content == "second").unwrap().id;
        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(second_id),
            &mut model,
        );

        assert_eq!(events.len(), 1);
        if let AppPanelViewInnerEvent::ShowToast { message } = &events[0] {
            assert!(message.contains("second"));
            assert!(!message.contains("first"));
            assert!(!message.contains("third"));
        } else {
            panic!("expected ShowToast event");
        }
    }

    // --- RecordDeleted 边界测试 ---

    #[test]
    fn handle_action_record_deleted_不存在的id无影响() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("keep this".to_string()).expect("add failed");
        let before_count = model.records().len();

        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(99999),
            &mut model,
        );

        assert_eq!(model.records().len(), before_count);
    }

    #[test]
    fn handle_action_record_deleted_删除后记录减少() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");
        model.add_record("c".to_string()).expect("add failed");

        let target_id = model.records().iter().find(|r| r.content == "b").unwrap().id;
        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(target_id),
            &mut model,
        );

        assert_eq!(model.records().len(), 2);
        assert!(model.records().iter().all(|r| r.content != "b"));
    }

    // --- ClearAllConfirmed 边界测试 ---

    #[test]
    fn handle_action_clear_all_confirmed_空列表安全处理() {
        let mut view = test_view();
        let mut model = test_model();

        view.confirm_clear_shown = true;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllConfirmed,
            &mut model,
        );

        assert!(model.records().is_empty());
        assert!(!view.confirm_clear_shown);
        // 空列表确认清空也应显示 toast
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn handle_action_clear_all_confirmed_toast内容正确() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("x".to_string()).expect("add failed");
        view.confirm_clear_shown = true;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllConfirmed,
            &mut model,
        );

        if let AppPanelViewInnerEvent::ShowToast { message } = &events[0] {
            assert_eq!(message, "已清空全部剪贴板历史");
        } else {
            panic!("expected ShowToast event");
        }
    }

    // --- ClearAllCancelled 边界测试 ---

    #[test]
    fn handle_action_clear_all_cancelled_弹窗未显示时仍安全() {
        let mut view = test_view();
        let mut model = test_model();

        assert!(!view.confirm_clear_shown);

        view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllCancelled,
            &mut model,
        );

        assert!(!view.confirm_clear_shown);
    }

    // --- 完整流程测试 ---

    #[test]
    fn 完整流程_请求清空到确认清空() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");
        assert_eq!(model.records().len(), 2);

        // 1. 点击"全部清空"按钮 → 显示确认弹窗
        view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllRequested,
            &mut model,
        );
        assert!(view.confirm_clear_shown);
        assert_eq!(model.records().len(), 2); // 记录尚未被删除

        // 2. 点击"确认"按钮 → 清空记录
        let events = view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllConfirmed,
            &mut model,
        );
        assert!(!view.confirm_clear_shown);
        assert!(model.records().is_empty());
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn 完整流程_请求清空到取消() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");

        // 1. 点击"全部清空"按钮 → 显示确认弹窗
        view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllRequested,
            &mut model,
        );
        assert!(view.confirm_clear_shown);

        // 2. 点击"取消"按钮 → 关闭弹窗，记录保留
        view.handle_clipboard_action(
            &ClipboardPageAction::ClearAllCancelled,
            &mut model,
        );
        assert!(!view.confirm_clear_shown);
        assert_eq!(model.records().len(), 1);
    }

    #[test]
    fn 完整流程_点击应用后删除记录() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("to apply then delete".to_string()).expect("add failed");
        let record_id = model.records()[0].id;

        // 1. 点击记录（应用）→ 显示 toast
        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(record_id),
            &mut model,
        );
        assert_eq!(events.len(), 1);
        assert!(model.records().iter().any(|r| r.id == record_id));

        // 2. 点击删除按钮 → 记录被删除
        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(record_id),
            &mut model,
        );
        assert!(model.records().is_empty());
    }
}
