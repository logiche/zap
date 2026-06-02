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
    /// 当前展开的剪贴板项 id（None 表示无展开项）
    pub selected_record_id: Option<i64>,
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
            selected_record_id: None,
        }
    }

    /// 切换当前子页面
    pub fn select_section(&mut self, section: AppPanelSection) {
        self.current_section = section;
    }

    /// 处理剪贴板页面 Action
    ///
    /// author logic
    /// date 2026-06-02
    pub fn handle_clipboard_action(
        &mut self,
        action: &ClipboardPageAction,
        model: &mut ClipboardHistoryModel,
    ) -> Vec<AppPanelViewInnerEvent> {
        let mut events = Vec::new();
        match action {
            ClipboardPageAction::RecordClicked(id) => {
                // 新语义：切换该 id 的展开/收起。仅当 id 真实存在时生效
                if model.records().iter().any(|r| r.id == *id) {
                    self.selected_record_id = if self.selected_record_id == Some(*id) {
                        None
                    } else {
                        Some(*id)
                    };
                }
            }
            ClipboardPageAction::RecordRightClicked { .. }
            | ClipboardPageAction::ContextMenuClosed
            | ClipboardPageAction::ContextMenuCopyRequested(_) => {
                // 纯状态层无副作用，由胶水层处理
            }
            ClipboardPageAction::RecordDeleted(id) => {
                let _ = model.delete(*id);
                // 若被删除的正是当前展开项，同步清空选中
                if self.selected_record_id == Some(*id) {
                    self.selected_record_id = None;
                }
            }
            ClipboardPageAction::ClearAllRequested => {
                self.confirm_clear_shown = true;
            }
            ClipboardPageAction::ClearAllConfirmed => {
                let _ = model.clear_all();
                self.confirm_clear_shown = false;
                self.selected_record_id = None;
                events.push(AppPanelViewInnerEvent::ShowToast {
                    message: "已清空全部剪贴板历史".to_string(),
                });
            }
            ClipboardPageAction::ClearAllCancelled => {
                self.confirm_clear_shown = false;
            }
            ClipboardPageAction::SyncRefreshRequested => {
                // 胶水层直接处理，此处无操作
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

    // --- RecordClicked 切换展开行为（新语义） ---

    #[test]
    fn record_clicked_未选中时点击展开() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("a".to_string()).expect("add failed");
        let id = model.records()[0].id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(id),
            &mut model,
        );

        assert_eq!(view.selected_record_id, Some(id));
        assert!(events.is_empty(), "RecordClicked 不应产生事件");
    }

    #[test]
    fn record_clicked_已选中时点击折叠() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("a".to_string()).expect("add failed");
        let id = model.records()[0].id;
        view.selected_record_id = Some(id);

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(id),
            &mut model,
        );

        assert_eq!(view.selected_record_id, None);
        assert!(events.is_empty());
    }

    #[test]
    fn record_clicked_切换到另一条() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("first".to_string()).expect("add failed");
        model.add_record("second".to_string()).expect("add failed");
        let first_id = model.records().iter().find(|r| r.content == "first").unwrap().id;
        let second_id = model.records().iter().find(|r| r.content == "second").unwrap().id;
        view.selected_record_id = Some(first_id);

        view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(second_id),
            &mut model,
        );

        assert_eq!(view.selected_record_id, Some(second_id));
    }

    #[test]
    fn record_clicked_不存在的id无副作用() {
        let mut view = test_view();
        let mut model = test_model();

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(99999),
            &mut model,
        );

        assert_eq!(view.selected_record_id, None);
        assert!(events.is_empty());
    }

    #[test]
    fn record_deleted_清空selected_record_id_if_match() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("to delete".to_string()).expect("add failed");
        let id = model.records()[0].id;
        view.selected_record_id = Some(id);

        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(id),
            &mut model,
        );

        assert_eq!(view.selected_record_id, None);
    }

    #[test]
    fn record_deleted_保留selected_record_id_if_other() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("keep".to_string()).expect("add failed");
        model.add_record("delete".to_string()).expect("add failed");
        let keep_id = model.records().iter().find(|r| r.content == "keep").unwrap().id;
        let delete_id = model.records().iter().find(|r| r.content == "delete").unwrap().id;
        view.selected_record_id = Some(keep_id);

        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(delete_id),
            &mut model,
        );

        assert_eq!(view.selected_record_id, Some(keep_id));
    }

    // --- 右键 / 上下文菜单 Action（纯状态层无副作用） ---

    #[test]
    fn record_right_clicked_无副作用() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("a".to_string()).expect("add failed");
        let id = model.records()[0].id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordRightClicked {
                record_id: id,
                position: warpui::geometry::vector::vec2f(0.0, 0.0),
            },
            &mut model,
        );

        assert!(events.is_empty());
        assert_eq!(view.selected_record_id, None);
        assert!(view.confirm_clear_shown == false);
    }

    #[test]
    fn context_menu_copy_requested_无副作用() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("a".to_string()).expect("add failed");
        let id = model.records()[0].id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::ContextMenuCopyRequested(id),
            &mut model,
        );

        assert!(events.is_empty());
        assert_eq!(view.selected_record_id, None);
    }

    #[test]
    fn context_menu_closed_无副作用() {
        let mut view = test_view();
        let mut model = test_model();
        view.selected_record_id = Some(42);

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::ContextMenuClosed,
            &mut model,
        );

        assert!(events.is_empty());
        assert_eq!(view.selected_record_id, Some(42), "ContextMenuClosed 不应改变 selected_record_id");
    }

    // --- 右键保留已选中状态（v1.1 新增） ---

    /// 右键一条记录时不应改变当前选中状态。
    /// 选中和右键是两个独立语义：用户可能既展开浏览某条记录，又对另一条进行复制/删除操作。
    #[test]
    fn record_right_clicked_保留已选中状态() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("expanded".to_string()).expect("add failed");
        model.add_record("right_clicked".to_string()).expect("add failed");
        let expanded_id = model.records().iter().find(|r| r.content == "expanded").unwrap().id;
        let right_clicked_id = model.records().iter().find(|r| r.content == "right_clicked").unwrap().id;
        view.selected_record_id = Some(expanded_id);

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordRightClicked {
                record_id: right_clicked_id,
                position: warpui::geometry::vector::vec2f(0.0, 0.0),
            },
            &mut model,
        );

        assert!(events.is_empty(), "RecordRightClicked 不应产生事件");
        assert_eq!(
            view.selected_record_id,
            Some(expanded_id),
            "右键另一条记录时，已展开项应保持展开"
        );
    }

    /// 右键一条不存在的记录 id 时不应改变任何状态。
    #[test]
    fn record_right_clicked_不存在的id无副作用() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("a".to_string()).expect("add failed");
        view.selected_record_id = Some(1);
        let before_selected = view.selected_record_id;

        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordRightClicked {
                record_id: 99999,
                position: warpui::geometry::vector::vec2f(10.0, 20.0),
            },
            &mut model,
        );

        assert!(events.is_empty());
        assert_eq!(view.selected_record_id, before_selected, "右键不存在的 id 不应改变选中状态");
        assert_eq!(model.records().len(), 1, "右键不存在的 id 不应影响记录列表");
    }

    /// 完整流程：选中 A → 右键 B → 关闭菜单 → A 仍被选中
    /// 验证右键生命周期不污染展开状态。
    #[test]
    fn 完整流程_选中A右键B关闭菜单A仍展开() {
        let mut view = test_view();
        let mut model = test_model();
        model.add_record("A".to_string()).expect("add failed");
        model.add_record("B".to_string()).expect("add failed");
        let a_id = model.records().iter().find(|r| r.content == "A").unwrap().id;
        let b_id = model.records().iter().find(|r| r.content == "B").unwrap().id;

        // 1. 选中 A
        view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(a_id),
            &mut model,
        );
        assert_eq!(view.selected_record_id, Some(a_id));

        // 2. 右键 B
        view.handle_clipboard_action(
            &ClipboardPageAction::RecordRightClicked {
                record_id: b_id,
                position: warpui::geometry::vector::vec2f(50.0, 50.0),
            },
            &mut model,
        );
        assert_eq!(view.selected_record_id, Some(a_id), "右键 B 不应改变 A 的展开状态");

        // 3. 关闭菜单
        view.handle_clipboard_action(
            &ClipboardPageAction::ContextMenuClosed,
            &mut model,
        );
        assert_eq!(view.selected_record_id, Some(a_id), "关闭菜单不应影响展开状态");
    }

    // --- 兼容旧用例的占位符（已不再适用的旧 RecordClicked 行为由 ContextMenuCopyRequested 承担） ---

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
        assert_eq!(view.selected_record_id, None);
    }

    // --- SelectSection 按钮 ---

    #[test]
    fn select_section_切换子页面() {
        let mut view = test_view();

        view.select_section(AppPanelSection::Clipboard);

        assert_eq!(view.current_section, AppPanelSection::Clipboard);
    }

    // --- RecordClicked（点击应用按钮）深入测试 ---

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
    fn 完整流程_点击展开后删除记录() {
        let mut view = test_view();
        let mut model = test_model();

        model.add_record("to expand then delete".to_string()).expect("add failed");
        let record_id = model.records()[0].id;

        // 1. 点击记录（展开）→ 设置 selected_record_id，不产生事件
        let events = view.handle_clipboard_action(
            &ClipboardPageAction::RecordClicked(record_id),
            &mut model,
        );
        assert!(events.is_empty());
        assert_eq!(view.selected_record_id, Some(record_id));
        assert!(model.records().iter().any(|r| r.id == record_id));

        // 2. 点击删除按钮 → 记录被删除，selected_record_id 同步清空
        view.handle_clipboard_action(
            &ClipboardPageAction::RecordDeleted(record_id),
            &mut model,
        );
        assert!(model.records().is_empty());
        assert_eq!(view.selected_record_id, None);
    }
}
