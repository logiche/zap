//! 应用面板 UI
//!
//! 提供应用面板的核心状态管理和子页面定义。
//! app/ 中的胶水层负责实现 PaneContent/BackingView trait 和实际渲染。
//!
//! author logic
//! date 2026-05-31

pub mod app_panel_view;
pub mod clipboard_page;
pub mod nav;

pub use app_panel_view::{AppPanelViewInner, AppPanelViewInnerEvent};
pub use clipboard_page::ClipboardPageAction;
pub use nav::AppPanelSection;
