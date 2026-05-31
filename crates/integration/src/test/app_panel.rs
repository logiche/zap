//! 应用面板 UI 集成测试
//!
//! 验证应用面板的打开/关闭、重复打开、多 tab 场景。
//!
//! author logic
//! date 2026-05-31

use warp::integration_testing::{
    step::new_step_with_default_assertions,
    tab::{assert_pane_title, assert_tab_title},
    terminal::wait_until_bootstrapped_single_pane_for_tab,
    view_getters::workspace_view,
    window::save_active_window_id,
};
use warp::workspace::WorkspaceAction;

use crate::builder::Builder;
use crate::test::{assert_tab_count, new_builder};

const WINDOW_ID_KEY: &str = "app_panel_window_id";

/// 测试打开和关闭应用面板
pub fn test_open_and_close_app_panel() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Open app panel")
                .with_action(move |app, _, data| {
                    let window_id = *data
                        .get(WINDOW_ID_KEY)
                        .expect("window_id not found");
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_tab_title(1, "应用"))
                .add_assertion(assert_pane_title(1, 0, "应用")),
        )
        .with_step(
            new_step_with_default_assertions("Close app panel tab")
                .with_action(move |app, _, data| {
                    let window_id = *data
                        .get(WINDOW_ID_KEY)
                        .expect("window_id not found");
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

/// 测试关闭后重新打开应用面板
pub fn test_reopen_app_panel() -> Builder {
    new_builder()
        .with_step(
            wait_until_bootstrapped_single_pane_for_tab(0)
                .add_assertion(save_active_window_id(WINDOW_ID_KEY)),
        )
        .with_step(
            new_step_with_default_assertions("Open app panel first time")
                .with_action(move |app, _, data| {
                    let window_id = *data
                        .get(WINDOW_ID_KEY)
                        .expect("window_id not found");
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_pane_title(1, 0, "应用")),
        )
        .with_step(
            new_step_with_default_assertions("Close app panel")
                .with_action(move |app, _, data| {
                    let window_id = *data
                        .get(WINDOW_ID_KEY)
                        .expect("window_id not found");
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
                    let window_id = *data
                        .get(WINDOW_ID_KEY)
                        .expect("window_id not found");
                    let workspace_view_id = workspace_view(app, window_id).id();
                    app.dispatch_typed_action(
                        window_id,
                        &[workspace_view_id],
                        &WorkspaceAction::ShowAppPanel,
                    );
                })
                .add_assertion(assert_tab_count(2))
                .add_assertion(assert_tab_title(1, "应用"))
                .add_assertion(assert_pane_title(1, 0, "应用")),
        )
}
