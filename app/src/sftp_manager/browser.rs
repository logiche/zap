//! SFTP 浏览器主视图
//!
//! 实现 BackingView trait，作为 pane 的核心视图组件。
//! author: logic
//! date: 2026-05-26

use std::path::PathBuf;

use warp_core::ui::appearance::Appearance;
use warpui::elements::{Element, Flex};
use warpui::{AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use crate::editor::{
    EditorView, SingleLineEditorOptions, TextColors, TextOptions,
};
use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::pane::view;
use crate::pane_group::{BackingView, PaneConfiguration, PaneEvent};

use super::types::{ConnectionState, Dialog, FileEntry, TransferTask};

/// SFTP 浏览器动作
#[derive(Debug, Clone)]
pub enum SftpBrowserAction {
    /// 导航到指定路径
    NavigateTo(PathBuf),
    /// 返回上级目录
    GoUp,
    /// 刷新当前目录
    Refresh,
    /// 上传文件
    UploadFile,
    /// 删除指定索引的条目
    DeleteEntry(usize),
    /// 重命名指定索引的条目
    RenameEntry(usize),
    /// 下载指定索引的条目
    DownloadEntry(usize),
    /// 新建文件夹
    NewFolder,
    /// 打开指定索引的条目（目录则进入，文件则下载）
    OpenEntry(usize),
    /// 选中指定索引的条目
    SelectEntry(usize),
    /// 关闭对话框
    CloseDialog,
    /// 确认删除
    ConfirmDelete,
    /// 确认重命名
    ConfirmRename,
    /// 确认新建文件夹
    ConfirmNewFolder,
    /// 弹出右键菜单
    ContextMenu(usize),
    /// 关闭右键菜单
    CloseContextMenu,
    /// 查看条目详情
    DetailsEntry(usize),
    /// 设置搜索过滤
    SetSearchFilter(String),
    /// 清除搜索过滤
    ClearSearchFilter,
    /// 返回上级
    NavigateUp,
    /// 删除选中条目
    DeleteSelected,
    /// 创建文件夹
    CreateFolder,
    /// 切换传输面板
    ToggleTransferPanel,
    /// 执行上传
    ExecuteUpload(String),
}

/// SFTP 浏览器视图
pub struct SftpBrowserView {
    pane_configuration: ModelHandle<PaneConfiguration>,
    focus_handle: Option<PaneFocusHandle>,
    /// 当前路径
    current_path: PathBuf,
    /// 文件条目列表
    entries: Vec<FileEntry>,
    /// 选中的条目索引
    selected_indices: Vec<usize>,
    /// 连接状态
    connection_state: ConnectionState,
    /// 当前打开的对话框
    active_dialog: Option<Dialog>,
    /// 传输任务列表
    transfers: Vec<TransferTask>,
    /// 传输面板是否展开
    transfer_panel_expanded: bool,
    /// 重命名编辑器
    rename_editor: ViewHandle<EditorView>,
    /// 新建文件夹编辑器
    new_folder_editor: ViewHandle<EditorView>,
}

impl SftpBrowserView {
    /// 创建新的 SFTP 浏览器视图
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let pane_configuration = ctx.add_model(|_ctx| PaneConfiguration::new("SFTP Browser"));
        let rename_editor = ctx.add_typed_action_view(|ctx| {
            let appearance = Appearance::as_ref(ctx);
            let theme = appearance.theme();
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_size_override: Some(appearance.ui_font_size()),
                    font_family_override: Some(appearance.monospace_font_family()),
                    text_colors_override: Some(TextColors {
                        default_color: theme.active_ui_text_color(),
                        disabled_color: theme.disabled_ui_text_color(),
                        hint_color: theme.disabled_ui_text_color(),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            EditorView::single_line(options, ctx)
        });
        let new_folder_editor = ctx.add_typed_action_view(|ctx| {
            let appearance = Appearance::as_ref(ctx);
            let theme = appearance.theme();
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_size_override: Some(appearance.ui_font_size()),
                    font_family_override: Some(appearance.monospace_font_family()),
                    text_colors_override: Some(TextColors {
                        default_color: theme.active_ui_text_color(),
                        disabled_color: theme.disabled_ui_text_color(),
                        hint_color: theme.disabled_ui_text_color(),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            EditorView::single_line(options, ctx)
        });

        Self {
            pane_configuration,
            focus_handle: None,
            current_path: PathBuf::from("/"),
            entries: Vec::new(),
            selected_indices: Vec::new(),
            connection_state: ConnectionState::Disconnected,
            active_dialog: None,
            transfers: Vec::new(),
            transfer_panel_expanded: false,
            rename_editor,
            new_folder_editor,
        }
    }

    /// 获取 pane 配置
    pub fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }
}

impl Entity for SftpBrowserView {
    type Event = PaneEvent;
}

impl TypedActionView for SftpBrowserView {
    type Action = SftpBrowserAction;
}

impl View for SftpBrowserView {
    fn ui_name() -> &'static str {
        "SftpBrowserView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        // 占位渲染：返回空 flex 布局
        Flex::column().finish()
    }
}

impl BackingView for SftpBrowserView {
    type PaneHeaderOverflowMenuAction = SftpBrowserAction;
    type CustomAction = ();
    type AssociatedData = ();

    /// 处理溢出菜单动作
    fn handle_pane_header_overflow_menu_action(
        &mut self,
        _action: &Self::PaneHeaderOverflowMenuAction,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    /// 关闭视图
    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(PaneEvent::Close);
    }

    /// 聚焦内容
    fn focus_contents(&mut self, _ctx: &mut ViewContext<Self>) {}

    /// 渲染头部内容
    fn render_header_content(
        &self,
        _ctx: &view::HeaderRenderContext<'_>,
        _app: &AppContext,
    ) -> view::HeaderContent {
        let title = format!("SFTP: {}", self.current_path.display());
        view::HeaderContent::simple(title)
    }

    /// 设置焦点句柄
    fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, _ctx: &mut ViewContext<Self>) {
        self.focus_handle = Some(focus_handle);
    }
}
