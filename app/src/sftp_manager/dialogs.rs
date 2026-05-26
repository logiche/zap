//! 对话框渲染组件
//!
//! 提供删除确认、重命名、新建文件夹、文件详情等对话框的渲染功能。
//! author: logic
//! date: 2026-05-26

use std::path::PathBuf;

use warp_core::ui::appearance::Appearance;
use warpui::elements::{
    Border, ChildView, Clipped, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Flex,
    Hoverable, MainAxisAlignment, ParentElement, Radius, Text,
};
use warpui::platform::Cursor;
use warpui::Element;
use warpui::ViewHandle;

use crate::editor::EditorView;
use crate::sftp_manager::browser::SftpBrowserAction;
use crate::sftp_manager::types::{format_size, Dialog, FileEntry};

/// 对话框宽度
const DIALOG_WIDTH: f32 = 360.0;
/// 对话框内边距
const DIALOG_PADDING: f32 = 16.0;
/// 按钮最小宽度
const BUTTON_MIN_WIDTH: f32 = 80.0;
/// 按钮高度
const BUTTON_HEIGHT: f32 = 32.0;

/// 弹窗外壳容器
///
/// 提供统一的背景色、圆角、边框和内边距。
fn dialog_shell(content: Box<dyn Element>, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    Container::new(content)
        .with_background(theme.surface_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.0)))
        .with_border(Border::all(1.0).with_border_fill(theme.surface_3()))
        .with_uniform_padding(DIALOG_PADDING)
        .finish()
}

/// 渲染按钮组件
///
/// is_accent 为 true 时使用 accent 色背景，否则使用 surface_2 背景。
fn render_button(
    label: &str,
    is_accent: bool,
    appearance: &Appearance,
    action: SftpBrowserAction,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();
    let bg = if is_accent {
        theme.accent()
    } else {
        theme.surface_2()
    };
    let text_color = if is_accent {
        theme.background()
    } else {
        theme.active_ui_text_color()
    };
    let label_owned = label.to_string();

    Hoverable::new(Default::default(), move |_| {
        let text_el = Text::new_inline(label_owned.clone(), ui_font, ui_font_size)
            .with_color(text_color.into())
            .finish();
        Container::new(
            ConstrainedBox::new(text_el)
                .with_min_width(BUTTON_MIN_WIDTH)
                .with_height(BUTTON_HEIGHT)
                .finish(),
        )
        .with_background(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

/// 渲染关闭/取消按钮
fn render_close_button(appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();
    let text_color = theme.active_ui_text_color();
    let bg = theme.surface_2();

    Hoverable::new(Default::default(), move |_| {
        let text_el = Text::new_inline(String::from("取消"), ui_font, ui_font_size)
            .with_color(text_color.into())
            .finish();
        Container::new(
            ConstrainedBox::new(text_el)
                .with_min_width(BUTTON_MIN_WIDTH)
                .with_height(BUTTON_HEIGHT)
                .finish(),
        )
        .with_background(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(|ctx, _, _| {
        ctx.dispatch_typed_action(SftpBrowserAction::CloseDialog);
    })
    .finish()
}

/// 渲染删除确认对话框
fn render_delete_confirm(paths: &[PathBuf], appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let sub_color = theme.sub_text_color(theme.background());
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    // 标题
    let title_el = Text::new_inline(String::from("确认删除"), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    // 描述
    let count = paths.len();
    let desc = if count == 1 {
        let name = paths[0]
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| paths[0].display().to_string());
        format!("确定要删除 \"{}\" 吗？此操作不可撤销。", name)
    } else {
        format!("确定要删除 {} 个项目吗？此操作不可撤销。", count)
    };
    let desc_el = Text::new_inline(desc, ui_font, ui_font_size)
        .with_color(sub_color.into())
        .finish();

    // 按钮
    let delete_btn = render_button("删除", true, appearance, SftpBrowserAction::ConfirmDelete);
    let cancel_btn = render_close_button(appearance);

    let buttons = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_alignment(MainAxisAlignment::End)
        .with_spacing(8.0)
        .with_child(Clipped::new(delete_btn).finish())
        .with_child(Clipped::new(cancel_btn).finish())
        .finish();

    let content = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.0)
        .with_child(title_el)
        .with_child(desc_el)
        .with_child(buttons)
        .finish();

    ConstrainedBox::new(dialog_shell(content, appearance))
        .with_width(DIALOG_WIDTH)
        .finish()
}

/// 渲染重命名对话框
fn render_rename(
    original_name: &str,
    editor: &ViewHandle<EditorView>,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let sub_color = theme.sub_text_color(theme.background());
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    // 标题
    let title_el = Text::new_inline(String::from("重命名"), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    // 当前名称提示
    let hint = format!("当前名称: {}", original_name);
    let hint_el = Text::new_inline(hint, ui_font, ui_font_size)
        .with_color(sub_color.into())
        .finish();

    // 编辑器
    let editor_el = Container::new(ChildView::new(editor).finish())
        .with_padding_left(8.0)
        .with_padding_right(8.0)
        .with_padding_top(4.0)
        .with_padding_bottom(4.0)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
        .with_background(theme.surface_2())
        .finish();

    // 按钮
    let confirm_btn = render_button("确定", true, appearance, SftpBrowserAction::ConfirmRename);
    let cancel_btn = render_close_button(appearance);

    let buttons = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_alignment(MainAxisAlignment::End)
        .with_spacing(8.0)
        .with_child(Clipped::new(confirm_btn).finish())
        .with_child(Clipped::new(cancel_btn).finish())
        .finish();

    let content = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.0)
        .with_child(title_el)
        .with_child(hint_el)
        .with_child(editor_el)
        .with_child(buttons)
        .finish();

    ConstrainedBox::new(dialog_shell(content, appearance))
        .with_width(DIALOG_WIDTH)
        .finish()
}

/// 渲染新建文件夹对话框
fn render_create_folder(
    editor: &ViewHandle<EditorView>,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    // 标题
    let title_el = Text::new_inline(String::from("新建文件夹"), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    // 编辑器
    let editor_el = Container::new(ChildView::new(editor).finish())
        .with_padding_left(8.0)
        .with_padding_right(8.0)
        .with_padding_top(4.0)
        .with_padding_bottom(4.0)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
        .with_background(theme.surface_2())
        .finish();

    // 按钮
    let confirm_btn = render_button("创建", true, appearance, SftpBrowserAction::ConfirmNewFolder);
    let cancel_btn = render_close_button(appearance);

    let buttons = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_alignment(MainAxisAlignment::End)
        .with_spacing(8.0)
        .with_child(Clipped::new(confirm_btn).finish())
        .with_child(Clipped::new(cancel_btn).finish())
        .finish();

    let content = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.0)
        .with_child(title_el)
        .with_child(editor_el)
        .with_child(buttons)
        .finish();

    ConstrainedBox::new(dialog_shell(content, appearance))
        .with_width(DIALOG_WIDTH)
        .finish()
}

/// 渲染单个属性行（标签 + 值）
fn detail_row(label: &str, value: &str, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let sub_color = theme.sub_text_color(theme.background());
    let text_color = theme.active_ui_text_color();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    let label_el = ConstrainedBox::new(
        Text::new_inline(label.to_string(), ui_font, ui_font_size)
            .with_color(sub_color.into())
            .finish(),
    )
    .with_width(80.0)
    .finish();

    let value_el = Text::new_inline(value.to_string(), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_spacing(8.0)
        .with_child(label_el)
        .with_child(value_el)
        .finish()
}

/// 渲染文件详情对话框
fn render_file_details(entry: &FileEntry, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    // 标题
    let title_el = Text::new_inline(String::from("文件详情"), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    // 类型
    let type_str = match entry.file_type {
        crate::sftp_manager::types::FileEntryType::File => "文件",
        crate::sftp_manager::types::FileEntryType::Directory => "目录",
        crate::sftp_manager::types::FileEntryType::Symlink => "符号链接",
        crate::sftp_manager::types::FileEntryType::Other => "其他",
    };

    // 构建属性行
    let mut rows = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(8.0);

    rows.add_child(detail_row("类型", type_str, appearance));
    rows.add_child(detail_row("大小", &format_size(entry.size), appearance));
    let modified = entry.modified.as_deref().unwrap_or("--");
    rows.add_child(detail_row("修改时间", modified, appearance));
    let permissions = entry.permissions.as_deref().unwrap_or("--");
    rows.add_child(detail_row("权限", permissions, appearance));
    rows.add_child(detail_row("路径", &entry.path.display().to_string(), appearance));

    // 关闭按钮
    let close_btn = render_close_button(appearance);

    let content = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.0)
        .with_child(title_el)
        .with_child(rows.finish())
        .with_child(close_btn)
        .finish();

    ConstrainedBox::new(dialog_shell(content, appearance))
        .with_width(DIALOG_WIDTH)
        .finish()
}

/// 渲染移动对话框
fn render_move_dialog(
    source: &PathBuf,
    target_dir: &PathBuf,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let sub_color = theme.sub_text_color(theme.background());
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    let title_el = Text::new_inline(String::from("移动文件"), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    let desc = format!(
        "将 {} 移动到 {}",
        source.display(),
        target_dir.display()
    );
    let desc_el = Text::new_inline(desc, ui_font, ui_font_size)
        .with_color(sub_color.into())
        .finish();

    let cancel_btn = render_close_button(appearance);

    let content = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.0)
        .with_child(title_el)
        .with_child(desc_el)
        .with_child(cancel_btn)
        .finish();

    ConstrainedBox::new(dialog_shell(content, appearance))
        .with_width(DIALOG_WIDTH)
        .finish()
}

/// 渲染覆盖确认对话框
fn render_overwrite_confirm(
    _source: &PathBuf,
    target: &PathBuf,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.active_ui_text_color();
    let sub_color = theme.sub_text_color(theme.background());
    let ui_font = appearance.ui_font_family();
    let ui_font_size = appearance.ui_font_size();

    let title_el = Text::new_inline(String::from("确认覆盖"), ui_font, ui_font_size)
        .with_color(text_color.into())
        .finish();

    let target_name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let desc = format!("目标文件 {} 已存在，是否覆盖？", target_name);
    let desc_el = Text::new_inline(desc, ui_font, ui_font_size)
        .with_color(sub_color.into())
        .finish();

    let confirm_btn =
        render_button("覆盖", true, appearance, SftpBrowserAction::ConfirmOverwrite);
    let cancel_btn = render_close_button(appearance);

    let buttons = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_alignment(MainAxisAlignment::End)
        .with_spacing(8.0)
        .with_child(Clipped::new(confirm_btn).finish())
        .with_child(Clipped::new(cancel_btn).finish())
        .finish();

    let content = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.0)
        .with_child(title_el)
        .with_child(desc_el)
        .with_child(buttons)
        .finish();

    ConstrainedBox::new(dialog_shell(content, appearance))
        .with_width(DIALOG_WIDTH)
        .finish()
}

/// 渲染对话框（主入口函数）
///
/// 根据对话框类型分发到对应的渲染函数。
pub fn render_dialog(
    dialog: &Dialog,
    rename_editor: &ViewHandle<EditorView>,
    new_folder_editor: &ViewHandle<EditorView>,
    appearance: &Appearance,
) -> Box<dyn Element> {
    match dialog {
        Dialog::DeleteConfirm { paths } => render_delete_confirm(paths, appearance),
        Dialog::Rename {
            original_name,
            ..
        } => render_rename(original_name, rename_editor, appearance),
        Dialog::CreateFolder { .. } => render_create_folder(new_folder_editor, appearance),
        Dialog::FileDetails { entry } => render_file_details(entry, appearance),
        Dialog::Move { source, target_dir } => {
            render_move_dialog(source, target_dir, appearance)
        }
        Dialog::OverwriteConfirm { source, target } => {
            render_overwrite_confirm(source, target, appearance)
        }
    }
}
