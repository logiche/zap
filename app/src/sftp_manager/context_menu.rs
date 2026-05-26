//! 右键上下文菜单渲染组件
//!
//! 提供文件条目的右键菜单渲染，包括打开、下载、重命名、删除、详情等操作。
//! author: logic
//! date: 2026-05-26

use warp_core::ui::appearance::Appearance;
use warpui::elements::{
    Container, CornerRadius, CrossAxisAlignment, Flex, Hoverable,
    MainAxisSize, ParentElement, Radius, Text,
};
use warpui::platform::Cursor;
use warpui::Element;

use crate::sftp_manager::browser::SftpBrowserAction;

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

/// 菜单项定义
struct MenuItem {
    /// 显示标签
    label: String,
    /// 关联动作
    action: SftpBrowserAction,
}

/// 构建文件右键菜单项列表
fn build_file_menu_items(entry_index: usize) -> Vec<MenuItem> {
    vec![
        MenuItem {
            label: String::from("打开"),
            action: SftpBrowserAction::OpenEntry(entry_index),
        },
        MenuItem {
            label: String::from("下载"),
            action: SftpBrowserAction::DownloadEntry(entry_index),
        },
        MenuItem {
            label: String::from("重命名"),
            action: SftpBrowserAction::RenameEntry(entry_index),
        },
        MenuItem {
            label: String::from("删除"),
            action: SftpBrowserAction::DeleteEntry(entry_index),
        },
        MenuItem {
            label: String::from("详细信息"),
            action: SftpBrowserAction::DetailsEntry(entry_index),
        },
    ]
}

/// 渲染单个菜单项
fn render_menu_item(
    label: &str,
    action: SftpBrowserAction,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let hover_bg = theme.surface_3();
    let default_bg = theme.surface_1();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();
    let label_owned = label.to_string();

    Hoverable::new(Default::default(), move |state| {
        let bg = if state.is_hovered() || state.is_clicked() {
            hover_bg
        } else {
            default_bg
        };
        let text_el = Text::new_inline(label_owned.clone(), ui_font, ui_font_size)
            .with_color(text_color.into())
            .finish();
        Container::new(text_el)
            .with_background(bg)
            .with_padding_left(12.0)
            .with_padding_right(12.0)
            .with_padding_top(6.0)
            .with_padding_bottom(6.0)
            .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

/// 渲染右键上下文菜单
pub fn render_context_menu(state: &ContextMenuState, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let menu_items = build_file_menu_items(state.entry_index);

    let mut col = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_main_axis_size(MainAxisSize::Min);

    for item in &menu_items {
        let el = render_menu_item(&item.label, item.action.clone(), appearance);
        col.add_child(el);
    }

    Container::new(col.finish())
        .with_background(theme.surface_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.0)))
        .with_padding_top(4.0)
        .with_padding_bottom(4.0)
        .finish()
}
