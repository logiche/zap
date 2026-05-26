//! 面包屑导航渲染组件
//!
//! author: logic
//! date: 2026-05-26

use std::path::PathBuf;

use warp_core::ui::appearance::Appearance;
use warpui::Element;

/// 渲染路径面包屑导航
pub fn render_breadcrumb(_current_path: &PathBuf, _appearance: &Appearance) -> Vec<Box<dyn Element>> {
    Vec::new()
}
