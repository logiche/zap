//! 应用面板 Pane 样式常量与辅助函数
//!
//! 集中管理布局尺寸、字号和可复用样式，遵循 ai_facts/view/style.rs 模式。
//!
//! author logic
//! date 2026-05-31

use crate::appearance::Appearance;

// --- 布局常量 ---

/// 侧边导航宽度
pub const SIDEBAR_WIDTH: f32 = 200.;
/// 侧边导航内边距
pub const SIDEBAR_PADDING: f32 = 8.;
/// 导航项左内边距
pub const NAV_ITEM_PADDING_LEFT: f32 = 16.;
/// 内容区内边距
pub const CONTENT_PADDING: f32 = 16.;
/// 记录行内边距
pub const RECORD_ROW_PADDING: f32 = 8.;
/// 搜索栏底部间距
pub const SEARCH_BAR_MARGIN_BOTTOM: f32 = 8.;
/// 清空按钮顶部间距
pub const CLEAR_BTN_MARGIN_TOP: f32 = 8.;
/// 删除图标尺寸
pub const DELETE_ICON_SIZE: f32 = 14.;
/// 小圆角半径
pub const CORNER_RADIUS_SMALL: f32 = 3.;
/// 确认弹窗宽度
pub const CONFIRM_DIALOG_WIDTH: f32 = 400.;
/// 确认弹窗按钮间距
pub const CONFIRM_BTN_MARGIN_LEFT: f32 = 12.;
/// 刷新按钮与搜索栏左边距
pub const REFRESH_BTN_MARGIN_LEFT: f32 = 8.;
/// 上下文菜单宽度
pub const CONTEXT_MENU_WIDTH: f32 = 160.;
/// 未选中行行高
pub const RECORD_ROW_HEIGHT: f32 = 48.;
/// 选中行内联展开后的最大行数
pub const EXPANDED_ROW_LINE_COUNT: usize = 6;
/// 选中行行高
pub const EXPANDED_ROW_HEIGHT: f32 = 152.;

// --- 字号辅助函数 ---

/// 获取时间戳文本字号
///
/// 使用比主 UI 字号小 2px 的尺寸，遵循项目的 detail/subtext 模式。
pub fn timestamp_font_size(appearance: &Appearance) -> f32 {
    appearance.ui_font_size() - 2.0
}
