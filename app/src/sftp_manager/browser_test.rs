//! SFTP 浏览器视图 UI 单元测试
//!
//! 验证视图状态管理、Action 处理逻辑。使用 App::test() + mock 平台，
//! 不依赖真实 SSH 连接（视图初始为 Disconnected 状态）。
//! author: logic
//! date: 2026-05-27

use std::path::PathBuf;

use warp_core::ui::appearance::Appearance;
use warpui::platform::WindowStyle;
use warpui::TypedActionView;

use crate::settings_view::keybindings::KeybindingChangedNotifier;
use crate::test_util::settings::initialize_settings_for_tests;

use super::browser::{SftpBrowserAction, SftpBrowserView};

/// 初始化测试所需的最小单例集合
fn initialize_app(app: &mut warpui::App) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| KeybindingChangedNotifier::mock());

    // SSH 管理器需要一个 SQLite 路径；使用临时文件，查询失败不 panic
    let temp_db = std::env::temp_dir().join("warp_sftp_test.sqlite");
    let _ = warp_ssh_manager::set_database_path(temp_db);
}

/// 创建 SftpBrowserView 并放入窗口
///
/// 视图初始状态为 Disconnected（无 SSH 连接），不影响 UI 状态逻辑测试。
fn create_view(app: &mut warpui::App) -> (warpui::WindowId, warpui::ViewHandle<SftpBrowserView>) {
    app.add_window(WindowStyle::NotStealFocus, |ctx| {
        SftpBrowserView::new("test-node".to_string(), ctx)
    })
}

// ============================================================
// 拖拽状态测试
// ============================================================

/// 验证 DragFilesEnter 设置 is_drag_hovering 为 true
#[test]
fn test_drag_files_enter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                view.is_drag_hovering,
                "DragFilesEnter 后 is_drag_hovering 应为 true"
            );
        });
    });
}

/// 验证 DragFilesLeave 设置 is_drag_hovering 为 false
#[test]
fn test_drag_files_leave() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // 先进入悬停状态
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });
        // 再离开
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesLeave, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                !view.is_drag_hovering,
                "DragFilesLeave 后 is_drag_hovering 应为 false"
            );
        });
    });
}

/// 验证 DragAndDropFiles 重置 is_drag_hovering
#[test]
fn test_drag_and_drop_resets_hover() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // 先进入悬停
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::DragFilesEnter, ctx);
        });
        // 释放文件（无 SFTP 连接，传输会失败但不崩溃）
        view.update(&mut app, |view, ctx| {
            view.handle_action(
                &SftpBrowserAction::DragAndDropFiles(vec![PathBuf::from("/tmp/test.txt")]),
                ctx,
            );
        });

        view.read(&app, |view, _| {
            assert!(
                !view.is_drag_hovering,
                "DragAndDropFiles 后 is_drag_hovering 应重置为 false"
            );
        });
    });
}

// ============================================================
// 选择状态测试
// ============================================================

/// 验证 SelectEntry 选中条目
#[test]
fn test_select_entry() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(0), ctx);
        });

        view.read(&app, |view, _| {
            assert!(view.selected.contains(&0), "SelectEntry(0) 后应选中索引 0");
        });
    });
}

/// 验证 SelectEntry 切换选中（单选模式：再次选中同一项仍保持选中）
#[test]
fn test_toggle_select_entry() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // 选中索引 2
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(2), ctx);
        });
        view.read(&app, |view, _| {
            assert!(view.selected.contains(&2));
        });

        // 选中索引 5 → 清除之前的，只保留 5
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SelectEntry(5), ctx);
        });
        view.read(&app, |view, _| {
            assert!(!view.selected.contains(&2), "SelectEntry(5) 后应取消选中 2");
            assert!(view.selected.contains(&5), "SelectEntry(5) 后应选中 5");
        });
    });
}

// ============================================================
// 搜索过滤测试
// ============================================================

/// 验证 SetSearchFilter 设置搜索文本
#[test]
fn test_set_search_filter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SetSearchFilter("txt".to_string()), ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(view.search_filter.as_deref(), Some("txt"));
        });
    });
}

/// 验证 ClearSearchFilter 清除搜索文本
#[test]
fn test_clear_search_filter() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        // 先设置
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::SetSearchFilter("log".to_string()), ctx);
        });
        // 再清除
        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::ClearSearchFilter, ctx);
        });

        view.read(&app, |view, _| {
            assert!(
                view.search_filter.is_none(),
                "ClearSearchFilter 后应为 None"
            );
        });
    });
}

// ============================================================
// 导航测试
// ============================================================

/// 验证在根目录 NavigateUp 不改变路径
#[test]
fn test_navigate_up_from_root() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            assert_eq!(view.current_path, PathBuf::from("/"));
        });

        view.update(&mut app, |view, ctx| {
            view.handle_action(&SftpBrowserAction::NavigateUp, ctx);
        });

        view.read(&app, |view, _| {
            assert_eq!(
                view.current_path,
                PathBuf::from("/"),
                "根目录 NavigateUp 应保持不变"
            );
        });
    });
}

// ============================================================
// 初始状态测试
// ============================================================

/// 验证视图初始状态正确
#[test]
fn test_initial_state() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let (_, view) = create_view(&mut app);

        view.read(&app, |view, _| {
            assert!(view.entries.is_empty(), "初始条目列表应为空");
            assert!(view.selected.is_empty(), "初始选中集合应为空");
            assert!(view.transfers.is_empty(), "初始传输列表应为空");
            assert!(view.search_filter.is_none(), "初始搜索过滤应为 None");
            assert!(!view.is_drag_hovering, "初始拖拽悬停应为 false");
            assert!(view.error_message.is_some(), "无 SSH 连接时应有错误消息");
        });
    });
}
