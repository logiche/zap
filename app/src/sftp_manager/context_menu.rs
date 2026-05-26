//! 右键上下文菜单渲染组件
//!
//! author: logic
//! date: 2026-05-26

use warp_core::ui::appearance::Appearance;
use warpui::Element;

/// 右键菜单状态
#[derive(Debug)]
pub struct ContextMenuState {
    /// 关联的文件条目索引
    pub entry_index: usize,
    /// 菜单弹出位置
    pub position: (f32, f32),
}

impl ContextMenuState {
    /// 创建新的右键菜单状态
    pub fn new(entry_index: usize, position: (f32, f32)) -> Self {
        Self { entry_index, position }
    }
}

/// 渲染右键上下文菜单
pub fn render_context_menu(_state: &ContextMenuState, _appearance: &Appearance) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
