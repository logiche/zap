//! SFTP 浏览器主视图
//!
//! 实现 BackingView trait，作为 pane 的核心视图组件。
//! 提供远程文件浏览、上传下载、目录导航等完整功能。
//! author: logic
//! date: 2026-05-26

use std::collections::HashSet;
use std::path::PathBuf;

use warp_core::ui::appearance::Appearance;
use warp_core::ui::icons::Icon;
use warp_ssh_manager::{KeychainSecretStore, SshRepository};
use warpui::elements::{
    Align, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container, CornerRadius,
    CrossAxisAlignment, Element, Fill, Flex, Hoverable, MainAxisAlignment, MainAxisSize,
    MouseStateHandle, ParentElement, Radius, ScrollbarWidth, Shrinkable, Text,
};
use warpui::platform::{Cursor, FilePickerConfiguration};
use warpui::{AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use crate::editor::{EditorView, Event as EditorEvent, SingleLineEditorOptions, TextColors, TextOptions};
use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::pane::view;
use crate::pane_group::{BackingView, PaneConfiguration, PaneEvent};

use super::context_menu::ContextMenuState;
use super::sftp_ops;
use super::types::{ConnectionState, Dialog, FileEntry, FileEntryType, TransferDirection, TransferTask};

/// 工具栏按钮尺寸
const TOOLBAR_BTN_SIZE: f32 = 28.0;
/// 工具栏图标尺寸
const TOOLBAR_ICON_SIZE: f32 = 16.0;
/// 工具栏间距
const TOOLBAR_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;

/// SFTP 浏览器动作
#[derive(Debug, Clone)]
pub enum SftpBrowserAction {
    /// 导航到指定路径
    NavigateTo(PathBuf),
    /// 返回上级目录
    GoUp,
    /// 后退（历史记录）
    GoBack,
    /// 前进（历史记录）
    GoForward,
    /// 刷新当前目录
    Refresh,
    /// 选中指定索引的条目
    SelectEntry(usize),
    /// 打开指定索引的条目（目录则进入，文件则下载）
    OpenEntry(usize),
    /// 删除指定索引的条目
    DeleteEntry(usize),
    /// 重命名指定索引的条目
    RenameEntry(usize),
    /// 下载指定索引的条目
    DownloadEntry(usize),
    /// 上传文件
    UploadFile,
    /// 新建文件夹
    NewFolder,
    /// 确认删除
    ConfirmDelete,
    /// 确认重命名
    ConfirmRename,
    /// 确认新建文件夹
    ConfirmNewFolder,
    /// 确认覆盖
    ConfirmOverwrite,
    /// 切换传输面板
    ToggleTransferPanel,
    /// 弹出右键菜单
    ContextMenu(usize),
    /// 关闭右键菜单
    CloseContextMenu,
    /// 关闭对话框
    CloseDialog,
    /// 查看条目详情
    DetailsEntry(usize),
    /// 设置搜索过滤
    SetSearchFilter(String),
    /// 清除搜索过滤
    ClearSearchFilter,
    /// 返回上级（键盘快捷键）
    NavigateUp,
    /// 删除选中条目（键盘快捷键）
    DeleteSelected,
    /// 创建文件夹（键盘快捷键）
    CreateFolder,
    /// 拖放文件上传
    DragAndDropFiles(Vec<PathBuf>),
    /// 执行上传
    ExecuteUpload(String),
}

/// SFTP 浏览器视图
pub struct SftpBrowserView {
    /// 关联的 SSH 服务器节点 ID
    node_id: String,
    /// pane 配置句柄
    pane_configuration: ModelHandle<PaneConfiguration>,
    /// 焦点句柄
    focus_handle: Option<PaneFocusHandle>,
    // ---- 连接 ----
    /// 连接状态
    connection: ConnectionState,
    /// SFTP 会话
    _session: Option<zap_sftp::SftpSession>,
    /// SFTP 操作通道
    sftp: Option<zap_sftp::Sftp>,
    // ---- 导航 ----
    /// 当前路径
    current_path: PathBuf,
    /// 当前目录文件条目
    entries: Vec<FileEntry>,
    /// 选中的条目索引集合
    selected: HashSet<usize>,
    /// 路径历史记录
    path_history: Vec<PathBuf>,
    /// 历史记录当前位置
    history_index: usize,
    // ---- 传输 ----
    /// 传输任务列表
    transfers: Vec<TransferTask>,
    /// 下一个传输任务 ID
    next_transfer_id: usize,
    /// 传输面板是否展开
    transfers_expanded: bool,
    // ---- UI 状态 ----
    /// 当前打开的对话框
    dialog: Option<Dialog>,
    /// 错误消息
    error_message: Option<String>,
    /// 是否正在加载
    is_loading: bool,
    /// 右键菜单状态
    context_menu: Option<ContextMenuState>,
    /// 搜索过滤文本
    search_filter: Option<String>,
    // ---- 鼠标句柄 ----
    /// 刷新按钮
    refresh_btn: MouseStateHandle,
    /// 上级目录按钮
    up_btn: MouseStateHandle,
    /// 后退按钮
    back_btn: MouseStateHandle,
    /// 前进按钮
    forward_btn: MouseStateHandle,
    /// 上传按钮
    upload_btn: MouseStateHandle,
    /// 新建文件夹按钮
    new_folder_btn: MouseStateHandle,
    // ---- 对话框编辑器 ----
    /// 重命名编辑器
    rename_editor: ViewHandle<EditorView>,
    /// 新建文件夹编辑器
    new_folder_editor: ViewHandle<EditorView>,
    // ---- 文件行鼠标句柄 ----
    /// 每行文件条目的鼠标状态句柄
    row_mouse_handles: Vec<MouseStateHandle>,
    // ---- 滚动 ----
    /// 滚动状态句柄
    scroll_state: ClippedScrollStateHandle,
}

impl SftpBrowserView {
    /// 创建新的 SFTP 浏览器视图
    pub fn new(node_id: String, ctx: &mut ViewContext<Self>) -> Self {
        let pane_configuration = ctx.add_model(|_ctx| PaneConfiguration::new("SFTP Browser"));
        let rename_editor = make_editor("Enter new name", ctx);
        let new_folder_editor = make_editor("Folder name", ctx);

        let mut me = Self {
            node_id,
            pane_configuration,
            focus_handle: None,
            connection: ConnectionState::Disconnected,
            _session: None,
            sftp: None,
            current_path: PathBuf::from("/"),
            entries: Vec::new(),
            selected: HashSet::new(),
            path_history: vec![PathBuf::from("/")],
            history_index: 0,
            transfers: Vec::new(),
            next_transfer_id: 1,
            transfers_expanded: false,
            dialog: None,
            error_message: None,
            is_loading: false,
            context_menu: None,
            search_filter: None,
            refresh_btn: MouseStateHandle::default(),
            up_btn: MouseStateHandle::default(),
            back_btn: MouseStateHandle::default(),
            forward_btn: MouseStateHandle::default(),
            upload_btn: MouseStateHandle::default(),
            new_folder_btn: MouseStateHandle::default(),
            rename_editor,
            new_folder_editor,
            row_mouse_handles: Vec::new(),
            scroll_state: ClippedScrollStateHandle::default(),
        };

        // 订阅重命名编辑器事件
        let rename_editor_handle = me.rename_editor.clone();
        ctx.subscribe_to_view(&rename_editor_handle, |me, _source, event, ctx| {
            match event {
                EditorEvent::Enter => {
                    me.handle_action(&SftpBrowserAction::ConfirmRename, ctx);
                }
                EditorEvent::Escape => {
                    me.dialog = None;
                    ctx.notify();
                }
                _ => {}
            }
        });

        // 订阅新建文件夹编辑器事件
        let new_folder_editor_handle = me.new_folder_editor.clone();
        ctx.subscribe_to_view(&new_folder_editor_handle, |me, _source, event, ctx| {
            match event {
                EditorEvent::Enter => {
                    me.handle_action(&SftpBrowserAction::ConfirmNewFolder, ctx);
                }
                EditorEvent::Escape => {
                    me.dialog = None;
                    ctx.notify();
                }
                _ => {}
            }
        });

        // 发起连接
        me.connect_to_server(ctx);

        me
    }

    /// 获取 pane 配置
    pub fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    /// 连接到 SSH 服务器并建立 SFTP 通道
    fn connect_to_server(&mut self, ctx: &mut ViewContext<Self>) {
        let node_id = self.node_id.clone();
        let result = warp_ssh_manager::with_conn(|c| {
            let server = SshRepository::get_server(c, &node_id)?;
            Ok(server)
        });

        match result {
            Ok(Some(server)) => {
                self.connection = ConnectionState::Connecting;
                self.is_loading = true;
                ctx.notify();

                let secret_store = KeychainSecretStore;
                match sftp_ops::connect_from_server(&server, &secret_store) {
                    Ok(session) => {
                        match session.sftp() {
                            Ok(sftp) => {
                                self.connection = ConnectionState::Connected;
                                self._session = Some(session);
                                self.sftp = Some(sftp);
                                self.is_loading = false;
                                // 列出根目录
                                self.refresh_dir(ctx);
                            }
                            Err(e) => {
                                self.connection = ConnectionState::Failed(format!(
                                    "创建 SFTP 通道失败: {e}"
                                ));
                                self.is_loading = false;
                                self.error_message = Some(format!("创建 SFTP 通道失败: {e}"));
                                ctx.notify();
                            }
                        }
                    }
                    Err(e) => {
                        self.connection = ConnectionState::Failed(e.to_string());
                        self.is_loading = false;
                        self.error_message = Some(e.to_string());
                        ctx.notify();
                    }
                }
            }
            Ok(None) => {
                self.connection = ConnectionState::Failed("未找到服务器配置".to_string());
                self.error_message = Some("未找到服务器配置".to_string());
                ctx.notify();
            }
            Err(e) => {
                self.connection = ConnectionState::Failed(format!("读取服务器配置失败: {e}"));
                self.error_message = Some(format!("读取服务器配置失败: {e}"));
                ctx.notify();
            }
        }
    }

    /// 刷新当前目录内容
    fn refresh_dir(&mut self, ctx: &mut ViewContext<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                self.error_message = Some("未连接到服务器".to_string());
                ctx.notify();
                return;
            }
        };

        self.is_loading = true;
        self.error_message = None;
        ctx.notify();

        let path = self.current_path.clone();
        match sftp_ops::list_dir(&sftp, &path) {
            Ok(mut entries) => {
                // 排序：目录在前，文件在后，各自按名称排序
                entries.sort_by(|a, b| {
                    match (a.file_type, b.file_type) {
                        (FileEntryType::Directory, FileEntryType::Directory) => {
                            a.name.to_lowercase().cmp(&b.name.to_lowercase())
                        }
                        (FileEntryType::Directory, _) => std::cmp::Ordering::Less,
                        (_, FileEntryType::Directory) => std::cmp::Ordering::Greater,
                        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    }
                });
                self.entries = entries;
                self.selected.clear();
                self.is_loading = false;
                // 同步行鼠标句柄
                self.sync_row_mouse_handles();
                ctx.notify();
            }
            Err(e) => {
                self.error_message = Some(format!("列出目录失败: {e}"));
                self.is_loading = false;
                ctx.notify();
            }
        }

        // 更新 pane 标题
        let title = format!("SFTP: {}", self.current_path.display());
        self.pane_configuration.update(ctx, |config, ctx| {
            config.set_title(title, ctx);
        });
    }

    /// 同步行鼠标句柄数量与条目数量一致
    fn sync_row_mouse_handles(&mut self) {
        while self.row_mouse_handles.len() < self.entries.len() {
            self.row_mouse_handles.push(MouseStateHandle::default());
        }
        self.row_mouse_handles.truncate(self.entries.len());
    }

    /// 导航到指定路径并更新历史记录
    fn navigate_to(&mut self, path: PathBuf, ctx: &mut ViewContext<Self>) {
        if path == self.current_path {
            return;
        }
        self.current_path = path;
        // 截断前进历史
        self.path_history.truncate(self.history_index + 1);
        self.path_history.push(self.current_path.clone());
        self.history_index = self.path_history.len() - 1;
        self.refresh_dir(ctx);
    }

    /// 返回上级目录
    fn go_up(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(parent) = self.current_path.parent() {
            let parent = parent.to_path_buf();
            if parent != self.current_path {
                self.navigate_to(parent, ctx);
            }
        }
    }

    /// 后退到历史记录中的上一个路径
    fn go_back(&mut self, ctx: &mut ViewContext<Self>) {
        if self.history_index > 0 {
            self.history_index -= 1;
            self.current_path = self.path_history[self.history_index].clone();
            self.refresh_dir(ctx);
        }
    }

    /// 前进到历史记录中的下一个路径
    fn go_forward(&mut self, ctx: &mut ViewContext<Self>) {
        if self.history_index < self.path_history.len() - 1 {
            self.history_index += 1;
            self.current_path = self.path_history[self.history_index].clone();
            self.refresh_dir(ctx);
        }
    }

    /// 打开指定索引的条目
    fn open_entry(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            match entry.file_type {
                FileEntryType::Directory | FileEntryType::Symlink => {
                    self.navigate_to(entry.path.clone(), ctx);
                }
                _ => {
                    self.download_entry(index, ctx);
                }
            }
        }
    }

    /// 弹出删除确认对话框
    fn delete_selected(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            let paths = if self.selected.contains(&index) {
                // 删除所有选中的
                self.selected
                    .iter()
                    .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
                    .collect()
            } else {
                vec![entry.path.clone()]
            };
            self.dialog = Some(Dialog::DeleteConfirm { paths });
            ctx.notify();
        }
    }

    /// 执行删除操作
    fn confirm_delete(&mut self, ctx: &mut ViewContext<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                self.error_message = Some("未连接到服务器".to_string());
                self.dialog = None;
                ctx.notify();
                return;
            }
        };

        let paths = match &self.dialog {
            Some(Dialog::DeleteConfirm { paths }) => paths.clone(),
            _ => {
                self.dialog = None;
                ctx.notify();
                return;
            }
        };

        for path in &paths {
            let result = if self.entries.iter().any(|e| {
                e.path == *path && matches!(e.file_type, FileEntryType::Directory)
            }) {
                sftp_ops::delete_dir_recursive(&sftp, path)
            } else {
                sftp_ops::delete_file(&sftp, path)
            };
            if let Err(e) = result {
                self.error_message = Some(format!("删除失败: {e}"));
                break;
            }
        }

        self.dialog = None;
        self.selected.clear();
        self.refresh_dir(ctx);
    }

    /// 创建下载传输任务
    fn download_entry(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            let local_path = dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(&entry.name);

            // 检查是否已存在同名文件
            if local_path.exists() {
                self.dialog = Some(Dialog::OverwriteConfirm {
                    source: entry.path.clone(),
                    target: local_path,
                });
                ctx.notify();
                return;
            }

            let task = TransferTask::new(
                self.next_transfer_id,
                entry.path.clone(),
                local_path,
                TransferDirection::Download,
                entry.size,
            );
            self.next_transfer_id += 1;
            self.transfers.push(task);
            ctx.notify();
        }
    }

    /// 显示条目详情对话框
    fn show_details(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            self.dialog = Some(Dialog::FileDetails {
                entry: entry.clone(),
            });
            ctx.notify();
        }
    }

    /// 弹出重命名对话框
    fn rename_entry(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            self.dialog = Some(Dialog::Rename {
                path: entry.path.clone(),
                original_name: entry.name.clone(),
            });
            // 将当前名称写入编辑器
            self.rename_editor
                .update(ctx, |e, ctx| e.set_buffer_text(&entry.name, ctx));
            ctx.notify();
        }
    }

    /// 渲染单个工具栏按钮
    fn render_toolbar_btn(
        &self,
        icon: Icon,
        handle: MouseStateHandle,
        action: SftpBrowserAction,
        _tooltip: &str,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let icon_color = theme.sub_text_color(theme.background());

        let icon_el = ConstrainedBox::new(icon.to_warpui_icon(icon_color).finish())
            .with_width(TOOLBAR_ICON_SIZE)
            .with_height(TOOLBAR_ICON_SIZE)
            .finish();

        Hoverable::new(handle, move |_| {
            Container::new(
                ConstrainedBox::new(
                    Container::new(icon_el)
                        .with_uniform_padding(6.0)
                        .finish(),
                )
                .with_width(TOOLBAR_BTN_SIZE)
                .with_height(TOOLBAR_BTN_SIZE)
                .finish(),
            )
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
            .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish()
    }

    /// 渲染工具栏
    fn render_toolbar(&self, appearance: &Appearance) -> Box<dyn Element> {
        let nav_buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TOOLBAR_SPACING)
            .with_child(self.render_toolbar_btn(
                Icon::ChevronLeft,
                self.back_btn.clone(),
                SftpBrowserAction::GoBack,
                "Back",
                appearance,
            ))
            .with_child(self.render_toolbar_btn(
                Icon::ChevronRight,
                self.forward_btn.clone(),
                SftpBrowserAction::GoForward,
                "Forward",
                appearance,
            ))
            .with_child(self.render_toolbar_btn(
                Icon::ArrowUp,
                self.up_btn.clone(),
                SftpBrowserAction::GoUp,
                "Up",
                appearance,
            ))
            .with_child(self.render_toolbar_btn(
                Icon::Refresh,
                self.refresh_btn.clone(),
                SftpBrowserAction::Refresh,
                "Refresh",
                appearance,
            ))
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        let action_buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TOOLBAR_SPACING)
            .with_child(self.render_toolbar_btn(
                Icon::UploadCloud,
                self.upload_btn.clone(),
                SftpBrowserAction::UploadFile,
                "Upload",
                appearance,
            ))
            .with_child(self.render_toolbar_btn(
                Icon::Plus,
                self.new_folder_btn.clone(),
                SftpBrowserAction::NewFolder,
                "New folder",
                appearance,
            ))
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(nav_buttons)
            .with_child(action_buttons)
            .finish()
    }

    /// 渲染面包屑导航
    fn render_breadcrumb(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background());

        let parts: Vec<Box<dyn Element>> = super::breadcrumb::render_breadcrumb(
            &self.current_path,
            appearance,
        );

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.0);

        // 添加根目录 "/" 作为可点击入口
        let root_text = Text::new_inline(
            "/".to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(text_color.into())
        .finish();
        row.add_child(Container::new(root_text).finish());

        for part in parts {
            row.add_child(part);
        }

        Container::new(row.finish())
            .with_padding_left(4.0)
            .with_padding_right(4.0)
            .with_padding_top(4.0)
            .with_padding_bottom(4.0)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
            .with_background(theme.surface_2())
            .finish()
    }

    /// 渲染连接状态（非连接时）
    fn render_connection_state(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background());

        let (msg, icon) = match &self.connection {
            ConnectionState::Connecting => (
                "Connecting...".to_string(),
                Icon::Loading,
            ),
            ConnectionState::Failed(err) => (err.clone(), Icon::AlertCircle),
            ConnectionState::Disconnected => (
                "Disconnected".to_string(),
                Icon::AlertCircle,
            ),
            ConnectionState::Connected => unreachable!(),
        };

        let icon_el = ConstrainedBox::new(icon.to_warpui_icon(text_color).finish())
            .with_width(24.0)
            .with_height(24.0)
            .finish();

        let text_el = Text::new_inline(msg, appearance.ui_font_family(), appearance.ui_font_size())
            .with_color(text_color.into())
            .finish();

        let content = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(12.0)
            .with_child(icon_el)
            .with_child(text_el)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Align::new(
            Container::new(content)
                .with_uniform_padding(24.0)
                .finish(),
        )
        .finish()
    }

    /// 渲染错误消息
    fn render_error(&self, appearance: &Appearance) -> Option<Box<dyn Element>> {
        let err = self.error_message.as_ref()?;
        let theme = appearance.theme();

        let text_el = Text::new_inline(
            err.clone(),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(theme.ui_error_color())
        .finish();

        Some(
            Container::new(text_el)
                .with_uniform_padding(8.0)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
                .finish(),
        )
    }

    /// 渲染文件列表
    fn render_file_list(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();

        // 过滤条目
        let filtered_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                self.search_filter
                    .as_ref()
                    .map_or(true, |filter| {
                        entry
                            .name
                            .to_lowercase()
                            .contains(&filter.to_lowercase())
                    })
            })
            .map(|(i, _)| i)
            .collect();

        if filtered_indices.is_empty() {
            let text_el = Text::new_inline(
                "This folder is empty".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish();

            return Align::new(
                Container::new(text_el)
                    .with_uniform_padding(24.0)
                    .finish(),
            )
            .finish();
        }

        // 表头
        let header = super::file_list::render_header(appearance);

        // 文件行
        let rows = super::file_list::render_file_rows(
            &self.entries,
            &self.selected,
            &self.row_mouse_handles,
            appearance,
        );

        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(header)
            .with_child(Shrinkable::new(1.0, rows).finish())
            .finish()
    }

    /// 渲染传输面板
    fn render_transfers(&self, appearance: &Appearance) -> Box<dyn Element> {
        super::transfer_panel::render_transfer_panel(
            &self.transfers,
            self.transfers_expanded,
            appearance,
        )
    }

    /// 渲染搜索栏
    fn render_search_bar(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background());

        let filter_text = self
            .search_filter
            .as_deref()
            .unwrap_or("")
            .to_string();

        let search_icon = ConstrainedBox::new(
            Icon::Search
                .to_warpui_icon(text_color)
                .finish(),
        )
        .with_width(14.0)
        .with_height(14.0)
        .finish();

        let text_el = if filter_text.is_empty() {
            Text::new_inline(
                "Search files...".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(theme.disabled_ui_text_color().into())
            .finish()
        } else {
            Text::new_inline(filter_text, appearance.ui_font_family(), appearance.ui_font_size())
                .with_color(text_color.into())
                .finish()
        };

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(4.0)
                .with_child(search_icon)
                .with_child(Shrinkable::new(1.0, text_el).finish())
                .finish(),
        )
        .with_padding_left(8.0)
        .with_padding_right(8.0)
        .with_padding_top(4.0)
        .with_padding_bottom(4.0)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
        .with_background(theme.surface_2())
        .finish()
    }

    /// 渲染加载中状态
    fn render_loading(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background());

        let icon_el = ConstrainedBox::new(
            Icon::Loading
                .to_warpui_icon(text_color)
                .finish(),
        )
        .with_width(24.0)
        .with_height(24.0)
        .finish();

        let text_el = Text::new_inline(
            "Loading...".to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(text_color.into())
        .finish();

        Align::new(
            Container::new(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(8.0)
                    .with_child(icon_el)
                    .with_child(text_el)
                    .with_main_axis_size(MainAxisSize::Min)
                    .finish(),
            )
            .with_uniform_padding(24.0)
            .finish(),
        )
        .finish()
    }
}

/// 构建重命名后的完整路径
fn build_rename_path(original_path: &PathBuf, new_name: &str) -> PathBuf {
    match original_path.parent() {
        Some(parent) => parent.join(new_name),
        None => PathBuf::from(new_name),
    }
}

/// 构建新建文件夹的完整路径
fn build_new_folder_path(parent_path: &PathBuf, folder_name: &str) -> PathBuf {
    parent_path.join(folder_name)
}

/// 构建上传后的远程路径
fn build_upload_remote_path(current_path: &PathBuf, local_file_name: &str) -> PathBuf {
    current_path.join(local_file_name)
}

impl Entity for SftpBrowserView {
    type Event = PaneEvent;
}

impl TypedActionView for SftpBrowserView {
    type Action = SftpBrowserAction;

    /// 处理所有 SFTP 浏览器动作
    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SftpBrowserAction::NavigateTo(path) => {
                self.navigate_to(path.clone(), ctx);
            }
            SftpBrowserAction::GoUp => {
                self.go_up(ctx);
            }
            SftpBrowserAction::GoBack => {
                self.go_back(ctx);
            }
            SftpBrowserAction::GoForward => {
                self.go_forward(ctx);
            }
            SftpBrowserAction::Refresh => {
                self.refresh_dir(ctx);
            }
            SftpBrowserAction::SelectEntry(index) => {
                let index = *index;
                self.selected.clear();
                self.selected.insert(index);
                ctx.notify();
            }
            SftpBrowserAction::OpenEntry(index) => {
                let index = *index;
                self.open_entry(index, ctx);
            }
            SftpBrowserAction::DeleteEntry(index) => {
                let index = *index;
                self.delete_selected(index, ctx);
            }
            SftpBrowserAction::RenameEntry(index) => {
                let index = *index;
                self.rename_entry(index, ctx);
            }
            SftpBrowserAction::DownloadEntry(index) => {
                let index = *index;
                self.download_entry(index, ctx);
            }
            SftpBrowserAction::UploadFile => {
                ctx.open_file_picker(
                    move |result, ctx: &mut ViewContext<SftpBrowserView>| match result {
                        Ok(paths) => {
                            for path in paths {
                                ctx.dispatch_typed_action(
                                    &SftpBrowserAction::ExecuteUpload(path),
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("sftp: file picker failed: {e}");
                        }
                    },
                    FilePickerConfiguration::new(),
                );
            }
            SftpBrowserAction::NewFolder => {
                self.dialog = Some(Dialog::CreateFolder {
                    parent_path: self.current_path.clone(),
                });
                self.new_folder_editor
                    .update(ctx, |e, ctx| e.set_buffer_text("", ctx));
                ctx.notify();
            }
            SftpBrowserAction::ConfirmDelete => {
                self.confirm_delete(ctx);
            }
            SftpBrowserAction::ConfirmRename => {
                if let Some(Dialog::Rename {
                    path: original_path,
                    ..
                }) = &self.dialog
                {
                    let new_name = self
                        .rename_editor
                        .as_ref(ctx)
                        .buffer_text(ctx);
                    let new_name = new_name.trim().to_string();
                    if new_name.is_empty() {
                        self.error_message = Some("名称不能为空".to_string());
                        ctx.notify();
                        return;
                    }
                    let new_path = build_rename_path(original_path, &new_name);

                    if let Some(sftp) = &self.sftp {
                        match sftp_ops::rename(sftp, original_path, &new_path) {
                            Ok(()) => {
                                self.dialog = None;
                                self.error_message = None;
                                self.refresh_dir(ctx);
                            }
                            Err(e) => {
                                self.error_message = Some(format!("重命名失败: {e}"));
                                ctx.notify();
                            }
                        }
                    }
                }
            }
            SftpBrowserAction::ConfirmNewFolder => {
                if let Some(Dialog::CreateFolder { parent_path }) = &self.dialog
                {
                    let folder_name = self
                        .new_folder_editor
                        .as_ref(ctx)
                        .buffer_text(ctx);
                    let folder_name = folder_name.trim().to_string();
                    if folder_name.is_empty() {
                        self.error_message = Some("文件夹名称不能为空".to_string());
                        ctx.notify();
                        return;
                    }
                    let folder_path =
                        build_new_folder_path(parent_path, &folder_name);

                    if let Some(sftp) = &self.sftp {
                        match sftp_ops::create_dir(sftp, &folder_path) {
                            Ok(()) => {
                                self.dialog = None;
                                self.error_message = None;
                                self.refresh_dir(ctx);
                            }
                            Err(e) => {
                                self.error_message = Some(format!("创建文件夹失败: {e}"));
                                ctx.notify();
                            }
                        }
                    }
                }
            }
            SftpBrowserAction::ConfirmOverwrite => {
                // 简化处理：关闭对话框，执行下载
                self.dialog = None;
                ctx.notify();
            }
            SftpBrowserAction::ToggleTransferPanel => {
                self.transfers_expanded = !self.transfers_expanded;
                ctx.notify();
            }
            SftpBrowserAction::ContextMenu(index) => {
                let index = *index;
                self.context_menu = Some(ContextMenuState::new(index, (0.0, 0.0)));
                self.selected.clear();
                self.selected.insert(index);
                ctx.notify();
            }
            SftpBrowserAction::CloseContextMenu => {
                self.context_menu = None;
                ctx.notify();
            }
            SftpBrowserAction::CloseDialog => {
                self.dialog = None;
                ctx.notify();
            }
            SftpBrowserAction::DetailsEntry(index) => {
                let index = *index;
                self.show_details(index, ctx);
            }
            SftpBrowserAction::SetSearchFilter(filter) => {
                self.search_filter = Some(filter.clone());
                ctx.notify();
            }
            SftpBrowserAction::ClearSearchFilter => {
                self.search_filter = None;
                ctx.notify();
            }
            SftpBrowserAction::NavigateUp => {
                self.go_up(ctx);
            }
            SftpBrowserAction::DeleteSelected => {
                if let Some(&index) = self.selected.iter().next() {
                    self.delete_selected(index, ctx);
                }
            }
            SftpBrowserAction::CreateFolder => {
                self.handle_action(&SftpBrowserAction::NewFolder, ctx);
            }
            SftpBrowserAction::DragAndDropFiles(paths) => {
                for local_path in paths {
                    let file_name = local_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let remote_path =
                        build_upload_remote_path(&self.current_path, &file_name);
                    let task = TransferTask::new(
                        self.next_transfer_id,
                        local_path.clone(),
                        remote_path,
                        TransferDirection::Upload,
                        0, // 大小在上传时确定
                    );
                    self.next_transfer_id += 1;
                    self.transfers.push(task);
                }
                ctx.notify();
            }
            SftpBrowserAction::ExecuteUpload(local_path_str) => {
                let local_path = PathBuf::from(local_path_str);
                let file_name = local_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let remote_path =
                    build_upload_remote_path(&self.current_path, &file_name);

                let task = TransferTask::new(
                    self.next_transfer_id,
                    local_path.clone(),
                    remote_path.clone(),
                    TransferDirection::Upload,
                    0,
                );
                self.next_transfer_id += 1;
                self.transfers.push(task);

                // 执行上传
                if let Some(sftp) = &self.sftp {
                    if let Err(e) =
                        sftp_ops::upload_file_streaming(sftp, &local_path, &remote_path, None)
                    {
                        self.error_message = Some(format!("上传失败: {e}"));
                    } else {
                        self.refresh_dir(ctx);
                    }
                }
                ctx.notify();
            }
        }
    }
}

impl View for SftpBrowserView {
    fn ui_name() -> &'static str {
        "SftpBrowserView"
    }

    /// 渲染完整 UI 布局
    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        // 1. 非连接状态显示连接状态
        if !matches!(self.connection, ConnectionState::Connected) {
            return Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(self.render_connection_state(appearance))
                .finish();
        }

        let mut col = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Max);

        // 2. 面包屑
        col.add_child(
            Container::new(self.render_breadcrumb(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_top(PANEL_PADDING)
                .finish(),
        );

        // 3. 工具栏
        col.add_child(
            Container::new(self.render_toolbar(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_top(4.0)
                .with_padding_bottom(4.0)
                .finish(),
        );

        // 4. 搜索栏
        col.add_child(
            Container::new(self.render_search_bar(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_bottom(4.0)
                .finish(),
        );

        // 5. 错误消息
        if let Some(error_el) = self.render_error(appearance) {
            col.add_child(
                Container::new(error_el)
                    .with_padding_left(PANEL_PADDING)
                    .with_padding_right(PANEL_PADDING)
                    .finish(),
            );
        }

        // 6. 加载中 / 文件列表
        if self.is_loading {
            col.add_child(Shrinkable::new(1.0, self.render_loading(appearance)).finish());
        } else {
            let file_list = self.render_file_list(appearance);
            let scrollbar_color = theme.disabled_text_color(theme.background()).into();
            let scrollbar_thumb_hover = theme.main_text_color(theme.background()).into();
            let scrollable = ClippedScrollable::vertical(
                self.scroll_state.clone(),
                file_list,
                ScrollbarWidth::Auto,
                scrollbar_color,
                scrollbar_thumb_hover,
                Fill::None,
            )
            .finish();
            col.add_child(Shrinkable::new(1.0, scrollable).finish());
        }

        // 7. 传输面板
        if !self.transfers.is_empty() {
            col.add_child(
                Container::new(self.render_transfers(appearance))
                    .with_padding_left(PANEL_PADDING)
                    .with_padding_right(PANEL_PADDING)
                    .with_padding_bottom(PANEL_PADDING)
                    .finish(),
            );
        }

        // 8. 右键菜单
        let mut main_content = col.finish();
        if let Some(ref cm_state) = self.context_menu {
            let menu_el = super::context_menu::render_context_menu(cm_state, appearance);
            main_content = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(main_content)
                .with_child(menu_el)
                .finish();
        }

        // 9. 对话框（覆盖层）
        if let Some(ref dialog) = self.dialog {
            let dialog_el = super::dialogs::render_dialog(
                dialog,
                &self.rename_editor,
                &self.new_folder_editor,
                appearance,
            );
            main_content = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(main_content)
                .with_child(dialog_el)
                .finish();
        }

        main_content
    }
}

impl BackingView for SftpBrowserView {
    type PaneHeaderOverflowMenuAction = SftpBrowserAction;
    type CustomAction = ();
    type AssociatedData = ();

    /// 处理溢出菜单动作
    fn handle_pane_header_overflow_menu_action(
        &mut self,
        action: &Self::PaneHeaderOverflowMenuAction,
        ctx: &mut ViewContext<Self>,
    ) {
        self.handle_action(action, ctx);
    }

    /// 关闭视图
    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        // 断开连接
        self._session = None;
        self.sftp = None;
        self.connection = ConnectionState::Disconnected;
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

/// 创建单行编辑器
fn make_editor(
    placeholder: &str,
    ctx: &mut ViewContext<SftpBrowserView>,
) -> ViewHandle<EditorView> {
    let placeholder = placeholder.to_string();
    ctx.add_typed_action_view(move |ctx| {
        let options = {
            let appearance = Appearance::as_ref(ctx);
            let theme = appearance.theme();
            SingleLineEditorOptions {
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
            }
        };
        let mut editor = EditorView::single_line(options, ctx);
        editor.set_placeholder_text(&placeholder, ctx);
        editor
    })
}
