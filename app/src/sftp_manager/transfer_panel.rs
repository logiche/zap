//! 传输面板渲染组件
//!
//! author: logic
//! date: 2026-05-26

use warp_core::ui::appearance::Appearance;
use warpui::Element;

use super::types::TransferTask;

/// 渲染文件传输面板
pub fn render_transfer_panel(
    _transfers: &[TransferTask],
    _is_expanded: bool,
    _appearance: &Appearance,
) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
