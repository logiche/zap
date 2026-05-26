//! 传输面板渲染组件
//!
//! 提供文件传输进度面板的渲染功能，包括传输方向图标、状态标签、进度条和传输列表。
//! author: logic
//! date: 2026-05-26

use warp_core::ui::appearance::Appearance;
use warpui::elements::{
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Flex, Hoverable,
    MainAxisSize, ParentElement, Radius, Shrinkable, Text,
};
use warpui::platform::Cursor;
use warpui::Element;

use crate::sftp_manager::browser::SftpBrowserAction;
use crate::sftp_manager::types::{TransferDirection, TransferState, TransferTask};
use crate::ui_components::icons::Icon;

/// 进度条高度
const PROGRESS_BAR_HEIGHT: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;

/// 渲染传输方向图标
fn render_direction_icon(direction: &TransferDirection, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let icon_color = theme.sub_text_color(theme.background());

    let icon = match direction {
        TransferDirection::Upload => Icon::UploadCloud,
        TransferDirection::Download => Icon::Download,
    };

    ConstrainedBox::new(icon.to_warpui_icon(icon_color).finish())
        .with_width(14.0)
        .with_height(14.0)
        .finish()
}

/// 渲染传输状态标签
fn render_state_label(state: &TransferState, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    let (label, color) = match state {
        TransferState::Pending => (String::from("等待中"), theme.sub_text_color(theme.background())),
        TransferState::InProgress => (String::from("传输中"), theme.accent()),
        TransferState::Completed => (String::from("已完成"), theme.ui_green_color().into()),
        TransferState::Failed(_) => (String::from("失败"), theme.ui_error_color().into()),
        TransferState::Cancelled => (String::from("已取消"), theme.sub_text_color(theme.background())),
    };

    Text::new_inline(label, ui_font, ui_font_size)
        .with_color(color.into())
        .finish()
}

/// 渲染进度条
fn render_progress_bar(progress: u8, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();

    if progress == 0 {
        return ConstrainedBox::new(
            Container::new(Flex::row().finish())
                .with_background(theme.surface_3())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(2.0)))
                .finish(),
        )
        .with_height(PROGRESS_BAR_HEIGHT)
        .finish();
    }

    // 进度填充
    let fill = ConstrainedBox::new(
        Container::new(Flex::row().finish())
            .with_background(theme.accent())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(2.0)))
            .finish(),
    )
    .with_width(progress as f32)
    .with_height(PROGRESS_BAR_HEIGHT)
    .finish();

    ConstrainedBox::new(
        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(fill)
                .finish(),
        )
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(2.0)))
        .finish(),
    )
    .with_height(PROGRESS_BAR_HEIGHT)
    .finish()
}

/// 渲染单个传输行
fn render_transfer_row(task: &TransferTask, appearance: &Appearance) -> Box<dyn Element> {
    // 方向图标
    let dir_icon = render_direction_icon(&task.direction, appearance);

    // 文件名
    let file_name = task
        .source_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let name_el = Text::new_inline(
        file_name,
        appearance.ui_font_family(),
        appearance.ui_font_size(),
    )
    .with_color(appearance.theme().active_ui_text_color().into())
    .finish();

    // 状态标签
    let state_el = render_state_label(&task.state, appearance);

    // 第一行：图标 + 文件名 + 状态
    let top_row = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_spacing(6.0)
        .with_child(dir_icon)
        .with_child(Shrinkable::new(1.0, name_el).finish())
        .with_child(state_el)
        .finish();

    let mut col = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(4.0)
        .with_child(top_row);

    // 进度条（仅传输中显示）
    if matches!(task.state, TransferState::InProgress) {
        let bar = render_progress_bar(task.progress_percent(), appearance);
        col.add_child(bar);
    }

    Container::new(col.finish())
        .with_padding_top(4.0)
        .with_padding_bottom(4.0)
        .finish()
}

/// 渲染文件传输面板（主入口）
///
/// 显示传输任务列表，面板折叠时只显示标题栏，展开时显示所有传输任务。
pub fn render_transfer_panel(
    transfers: &[TransferTask],
    is_expanded: bool,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    // 标题栏
    let count = transfers.len();
    let title_text = format!("传输 ({})", count);

    let toggle_icon = if is_expanded {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };

    let header = Hoverable::new(Default::default(), move |_| {
        let toggle_icon_el = ConstrainedBox::new(
            toggle_icon.to_warpui_icon(text_color).finish(),
        )
        .with_width(14.0)
        .with_height(14.0)
        .finish();

        let title_el = Text::new_inline(title_text.clone(), ui_font, ui_font_size)
            .with_color(text_color.into())
            .finish();

        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(6.0)
            .with_child(toggle_icon_el)
            .with_child(title_el)
            .with_main_axis_size(MainAxisSize::Max)
            .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(|ctx, _, _| {
        ctx.dispatch_typed_action(SftpBrowserAction::ToggleTransferPanel);
    })
    .finish();

    let mut col = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_child(header);

    if is_expanded {
        for task in transfers {
            let row = render_transfer_row(task, appearance);
            col.add_child(row);
        }
    }

    Container::new(col.finish())
        .with_uniform_padding(PANEL_PADDING)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.0)))
        .with_background(theme.surface_2())
        .finish()
}
