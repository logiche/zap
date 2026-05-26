//! 文件列表渲染组件
//!
//! author: logic
//! date: 2026-05-26

use std::collections::HashSet;

use warp_core::ui::appearance::Appearance;
use warpui::elements::MouseStateHandle;
use warpui::Element;

use super::types::FileEntry;

/// 渲染文件列表头部
pub fn render_header(_appearance: &Appearance) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}

/// 渲染文件行列表
pub fn render_file_rows(
    _entries: &[FileEntry],
    _selected: &HashSet<usize>,
    _mouse_handles: &[MouseStateHandle],
    _appearance: &Appearance,
) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
