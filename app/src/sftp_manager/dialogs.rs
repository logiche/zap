//! 对话框渲染组件
//!
//! author: logic
//! date: 2026-05-26

use warp_core::ui::appearance::Appearance;
use warpui::Element;

use crate::editor::EditorView;
use warpui::ViewHandle;

use super::types::Dialog;

/// 渲染对话框
pub fn render_dialog(
    _dialog: &Dialog,
    _rename_editor: &ViewHandle<EditorView>,
    _new_folder_editor: &ViewHandle<EditorView>,
    _appearance: &Appearance,
) -> Box<dyn Element> {
    warpui::elements::Flex::column().finish()
}
