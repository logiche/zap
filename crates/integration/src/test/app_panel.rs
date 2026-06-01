//! 应用面板 UI 集成测试
//!
//! 验证应用面板的所有按钮交互：打开/关闭、点击记录（应用）、
//! 删除记录、全部清空、确认/取消清空、搜索过滤、重复打开。
//!
//! author logic
//! date 2026-05-31

use app_panel::ClipboardPageAction;
use clipboard_history::ClipboardHistoryModel;
use warp::integration_testing::{
    AppPanelAction, AppPanelView,
    step::new_step_with_default_assertions,
    tab::{assert_pane_title, assert_tab_title},
    terminal::wait_until_bootstrapped_single_pane_for_tab,
    view_getters::workspace_view,
    view_of_type,
    window::save_active_window_id,
};
use warp::workspace::WorkspaceAction;
use warpui::integration::AssertionOutcome;
use warpui::SingletonEntity;

use crate::builder::Builder;
use crate::test::{assert_tab_count, new_builder};

const WINDOW_ID_KEY: &str = "app_panel_window_id";

/// 获取 AppPanelView 的 view id
fn app_panel_view_id(app: &warpui::App, window_id: warpui::WindowId) -> warpui::EntityId {
    let view: warpui::ViewHandle<AppPanelView> = view_of_type(app, window_id, 0);
    view.id()
}

/// 获取记录数量
fn get_record_count(app: &warpui::App) -> usize {
    let model = ClipboardHistoryModel::handle(app);
    model.read(app, |m: &ClipboardHistoryModel, _| m.records().len())
}

/// 获取指定索引的记录 ID
fn get_record_id_at(app: &warpui::App, index: usize) -> i64 {
    let model = ClipboardHistoryModel::handle(app);
    model.read(app, |m: &ClipboardHistoryModel, _| m.records()[index].id)
}

/// 通过内容查找记录 ID
fn find_record_id(app: &warpui::App, content: &str) -> Option<i64> {
    let model = ClipboardHistoryModel::handle(app);
    model.read(app, |m: &ClipboardHistoryModel, _| {
        m.records().iter().find(|r| r.content == content).map(|r| r.id)
    })
}

/// 添加一条测试记录
fn add_test_record(app: &mut warpui::App, content: &str) {
    let model = ClipboardHistoryModel::handle(app);
    model.update(app, |model: &mut ClipboardHistoryModel, _ctx| {
        let _ = model.add_record(content.to_string());
    });
}

/// 从 data 获取 window_id
macro_rules! get_window_id {
    ($data:expr) => {
        *$data.get(WINDOW_ID_KEY).expect("window_id not found")
    };
}

// ====================================================================
// 测试 1：打开和关闭应用面板
// ====================================================================

pub fn test_open_and_close_app_panel() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Open app panel")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_tab_title(1, "app-panel-title"))
                .add_assertion(assert_pane_title(1, 0, "app-panel-title")),
        )
        .with_step(
            new_step_with_default_assertions("Close app panel tab")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::CloseTab(1),
                    );
                })
                .add_assertion(assert_tab_count(1)),
        )
}

// ====================================================================
// 测试 2：关闭后重新打开
// ====================================================================

pub fn test_reopen_app_panel() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Open app panel first time")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_pane_title(1, 0, "app-panel-title")),
        )
        .with_step(
            new_step_with_default_assertions("Close app panel")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::CloseTab(1),
                    );
                })
                .add_assertion(assert_tab_count(1)),
        )
        .with_step(
            new_step_with_default_assertions("Reopen app panel")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_tab_title(1, "app-panel-title"))
                .add_assertion(assert_pane_title(1, 0, "app-panel-title")),
        )
}

// ====================================================================
// 测试 3：重复打开不创建多个面板
// ====================================================================

pub fn test_duplicate_open_does_not_create_extra_tabs() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Open app panel")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2)),
        )
        .with_step(
            new_step_with_default_assertions("Click app panel button again")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2)),
        )
}

// ====================================================================
// 测试 4：点击记录行（应用/复制）
// ====================================================================

pub fn test_click_record_to_apply() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Add record and open panel")
                .with_action(move |app, _, data| {
                    add_test_record(app, "hello world");
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2)),
        )
        .with_step(
            new_step_with_default_assertions("Click record to apply")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let record_id = get_record_id_at(app, 0);
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::RecordClicked(record_id)),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 1, "record should still exist after apply");
                    AssertionOutcome::Success
                }),
        )
}

// ====================================================================
// 测试 5：删除记录
// ====================================================================

pub fn test_delete_record() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Add records and open panel")
                .with_action(move |app, _, data| {
                    add_test_record(app, "record to keep");
                    add_test_record(app, "record to delete");
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 2, "should have 2 records");
                    AssertionOutcome::Success
                }),
        )
        .with_step(
            new_step_with_default_assertions("Delete one record")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let record_id = find_record_id(app, "record to delete")
                        .expect("should find record");
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::RecordDeleted(record_id)),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 1, "should have 1 record after delete");
                    AssertionOutcome::Success
                }),
        )
}

// ====================================================================
// 测试 6：全部清空 → 确认
// ====================================================================

pub fn test_clear_all_confirmed() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Add records and open panel")
                .with_action(move |app, _, data| {
                    add_test_record(app, "first record");
                    add_test_record(app, "second record");
                    add_test_record(app, "third record");
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 3);
                    AssertionOutcome::Success
                }),
        )
        .with_step(
            new_step_with_default_assertions("Request clear all")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::ClearAllRequested),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 3, "records still exist after request");
                    AssertionOutcome::Success
                }),
        )
        .with_step(
            new_step_with_default_assertions("Confirm clear all")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::ClearAllConfirmed),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 0, "all records should be cleared");
                    AssertionOutcome::Success
                }),
        )
}

// ====================================================================
// 测试 7：全部清空 → 取消
// ====================================================================

pub fn test_clear_all_cancelled() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Add record and open panel")
                .with_action(move |app, _, data| {
                    add_test_record(app, "important data");
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 1);
                    AssertionOutcome::Success
                }),
        )
        .with_step(
            new_step_with_default_assertions("Request and cancel clear all")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::ClearAllRequested),
                    );
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::ClearAllCancelled),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 1, "record preserved after cancel");
                    AssertionOutcome::Success
                }),
        )
}

// ====================================================================
// 测试 8：搜索过滤
// ====================================================================

pub fn test_search_filters_records() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Add records and verify search")
                .with_action(move |app, _, data| {
                    add_test_record(app, "Rust programming");
                    add_test_record(app, "Python scripting");
                    add_test_record(app, "Rust async await");
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(move |app, _window_id| {
                    let model = ClipboardHistoryModel::handle(app);
                    model.read(app, |m: &ClipboardHistoryModel, _| {
                        assert_eq!(m.search("Rust").len(), 2, "'Rust' should match 2");
                        assert_eq!(m.search("rust").len(), 2, "case-insensitive");
                        assert_eq!(m.search("Python").len(), 1, "'Python' should match 1");
                        assert_eq!(m.search("nonexistent").len(), 0, "no match");
                        assert_eq!(m.search("").len(), 3, "empty returns all");
                    });
                    AssertionOutcome::Success
                }),
        )
}

// ====================================================================
// 测试 9：完整交互流程
// ====================================================================

pub fn test_full_interaction_flow() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Open panel with records")
                .with_action(move |app, _, data| {
                    add_test_record(app, "content A");
                    add_test_record(app, "content B");
                    let window_id = get_window_id!(data);
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 2);
                    AssertionOutcome::Success
                }),
        )
        // 应用 content B
        .with_step(
            new_step_with_default_assertions("Apply content B")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let record_id = get_record_id_at(app, 0);
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::RecordClicked(record_id)),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 2, "apply does not delete");
                    AssertionOutcome::Success
                }),
        )
        // 删除 content A
        .with_step(
            new_step_with_default_assertions("Delete content A")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let record_id = find_record_id(app, "content A").expect("not found");
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::RecordDeleted(record_id)),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 1, "1 after delete");
                    AssertionOutcome::Success
                }),
        )
        // 全部清空 → 确认
        .with_step(
            new_step_with_default_assertions("Clear all and confirm")
                .with_action(move |app, _, data| {
                    let window_id = get_window_id!(data);
                    let view_id = app_panel_view_id(app, window_id);
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::ClearAllRequested),
                    );
                    app.dispatch_typed_action(
                        window_id,
                        &[view_id],
                        &AppPanelAction::Clipboard(ClipboardPageAction::ClearAllConfirmed),
                    );
                })
                .add_assertion(move |app, _window_id| {
                    assert_eq!(get_record_count(app), 0, "all cleared");
                    AssertionOutcome::Success
                }),
        )
}
