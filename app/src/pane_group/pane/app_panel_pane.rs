//! 应用面板 Pane 胶水层
//!
//! 桥接 app_panel crate 的 AppPanelViewInner 与 app/ 内部的 PaneContent/BackingView trait。
//!
//! author logic
//! date 2026-05-31

use app_panel::clipboard_page::ClipboardPageAction;
use app_panel::nav::AppPanelSection;
use app_panel::AppPanelViewInner;
use chrono::Utc;
use clipboard_history::ClipboardHistoryModel;
use pathfinder_color::ColorU;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use warpui::clipboard::ClipboardContent;
use warpui::elements::{
    Align, Border, ChildView, ConstrainedBox, Container, CornerRadius,
    CrossAxisAlignment, Dismiss, Element, Expanded, Fill, Flex, Hoverable,
    List, ListState, MainAxisAlignment, MainAxisSize, MouseStateHandle,
    OffsetPositioning, ParentElement, PositionedElementAnchor, PositionedElementOffsetBounds,
    Radius, Rect, SavePosition, Scrollable, ScrollbarWidth, ScrollableElement,
    Shrinkable, Stack, Text,
};
use warpui::platform::Cursor;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Entity, ModelHandle, SingletonEntity as _, TypedActionView, View, ViewContext,
    ViewHandle, WeakViewHandle, WindowId,
};
use warp_core::ui::icons::Icon;

use crate::menu::{Event as MenuEvent, Menu, MenuItemFields};

use crate::app_state::{AppPanelPaneSnapshot, LeafContents};
use crate::appearance::Appearance;
use crate::editor::{EditorView, PropagateAndNoOpNavigationKeys, SingleLineEditorOptions, TextOptions};
use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::pane::app_panel_pane_manager::AppPanelPaneManager;
use crate::pane_group::pane::IPaneType;
use crate::pane_group::pane::view;
use crate::search_bar::SearchBar;
use crate::ui_components::dialog::{dialog_styles, Dialog};
use crate::ui_components::icons;
use crate::ui_components::spinner::{BrailleSpinner, SpinnerStateHandle};
use crate::view_components::action_button::{ActionButton, DangerPrimaryTheme, DangerSecondaryTheme, NakedTheme};
use crate::view_components::DismissibleToast;
use crate::ToastStack;

use super::{
    BackingView, DetachType, PaneConfiguration, PaneContent, PaneEvent, PaneGroup,
    PaneId, ShareableLink, ShareableLinkError,
};

// --- AppPanelAction ---

/// 应用面板的 Action
#[derive(Clone, Debug)]
pub enum AppPanelAction {
    /// 切换子页面
    SelectSection(AppPanelSection),
    /// 剪贴板页面事件
    Clipboard(ClipboardPageAction),
}

// --- AppPanelView ---

/// 单条剪贴板记录在列表中需要的数据
#[derive(Clone)]
struct ClipboardRecordRow {
    /// 数据库 id
    id: i64,
    /// 单行预览文本
    preview: String,
    /// 完整内容（用于展开时多行显示）
    content: String,
    /// 创建时间
    created_at: chrono::DateTime<chrono::Utc>,
}

/// 上下文菜单元数据：指向哪条记录、相对面板内容原点的偏移
#[derive(Clone)]
struct ClipboardContextMenuState {
    record_id: i64,
    offset: warpui::geometry::vector::Vector2F,
}

/// 应用面板 View（实现 BackingView）
pub struct AppPanelView {
    inner: AppPanelViewInner,
    pane_configuration: ModelHandle<PaneConfiguration>,
    focus_handle: Option<PaneFocusHandle>,
    nav_hover_states: Vec<MouseStateHandle>,
    /// 搜索输入框
    search_editor: ViewHandle<EditorView>,
    /// 搜索栏包装
    search_bar: ViewHandle<SearchBar>,
    /// 全部清空按钮
    clear_all_button: ViewHandle<ActionButton>,
    /// 确认弹窗取消按钮
    confirm_cancel_button: ViewHandle<ActionButton>,
    /// 确认弹窗清空按钮
    confirm_delete_button: ViewHandle<ActionButton>,
    /// 刷新同步按钮
    refresh_button: ViewHandle<ActionButton>,
    /// 变高行 List 状态（render_fn 闭包 'static，由构造时注入）
    list_state: ListState<()>,
    /// 当前已注入到 list_state 的 item 数量（用于 add/remove 协调）
    rendered_count: Cell<usize>,
    /// 当前过滤后用于渲染的剪贴板行（render_fn 闭包通过 Rc 共享）
    rows: Rc<RefCell<Vec<ClipboardRecordRow>>>,
    /// 内容区 SavePosition id（供右键回调计算菜单偏移）
    content_position_id: String,
    /// 滚动条状态
    scroll_state: Arc<Mutex<warpui::elements::ScrollState>>,
    /// Spinner 状态
    spinner_handle: SpinnerStateHandle,
    /// 上下文菜单视图句柄
    context_menu: ViewHandle<Menu<AppPanelAction>>,
    /// 当前打开的上下文菜单状态（None 表示未打开）
    context_menu_state: Option<ClipboardContextMenuState>,
}

impl Entity for AppPanelView {
    type Event = PaneEvent;
}

impl View for AppPanelView {
    fn ui_name() -> &'static str {
        "AppPanelView"
    }

    fn render(&self, ctx: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(ctx);
        let theme = appearance.theme();

        // 从搜索编辑器读取实时文本
        let search_term = self.search_editor.as_ref(ctx).buffer_text(ctx);
        let model = ClipboardHistoryModel::handle(ctx);
        let records = model.as_ref(ctx).records();
        let is_syncing = model.as_ref(ctx).is_syncing();
        let filtered: Vec<_> = if search_term.is_empty() {
            records.iter().collect()
        } else {
            let query = search_term.to_lowercase();
            records.iter().filter(|r| r.content.to_lowercase().contains(&query)).collect()
        };

        // --- 侧边导航 ---
        let mut nav_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min);

        for (idx, section) in AppPanelSection::all().iter().enumerate() {
            let hover = self.nav_hover_states[idx].clone();
            let is_active = *section == self.inner.current_section;
            let label = match section {
                AppPanelSection::Clipboard => crate::t!("app-panel-nav-clipboard"),
            };
            let section = *section;

            let item = appearance
                .ui_builder()
                .button(
                    if is_active { ButtonVariant::Accent } else { ButtonVariant::Text },
                    hover,
                )
                .with_text_label(label)
                .with_style(
                    UiComponentStyles::default()
                        .set_border_width(0.)
                        .set_margin(Coords::default().left(super::app_panel_style::NAV_ITEM_PADDING_LEFT))
                        .set_padding(Coords::uniform(super::app_panel_style::SIDEBAR_PADDING)),
                )
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(AppPanelAction::SelectSection(section));
                })
                .finish();

            nav_column = nav_column.with_child(item);
        }

        let sidebar = ConstrainedBox::new(
            Container::new(nav_column.finish())
                .with_border(Border::right(1.).with_border_fill(theme.outline()))
                .with_uniform_padding(super::app_panel_style::SIDEBAR_PADDING)
                .finish(),
        )
        .with_width(super::app_panel_style::SIDEBAR_WIDTH)
        .finish();

        // --- 搜索框行（搜索栏 + 刷新按钮/Spinner） ---
        let detail_color: ColorU = theme.nonactive_ui_text_color().into_solid();
        let ui_font_family = appearance.ui_font_family();
        let ui_font_size = appearance.ui_font_size();

        let search_bar_element = ChildView::new(&self.search_bar).finish();

        let refresh_or_spinner: Box<dyn Element> = if is_syncing {
            Box::new(BrailleSpinner::new(
                ui_font_family,
                ui_font_size,
                detail_color,
                self.spinner_handle.clone(),
            ))
        } else {
            ChildView::new(&self.refresh_button).finish()
        };

        let search_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(Expanded::new(1.0, search_bar_element).finish())
            .with_child(
                Container::new(refresh_or_spinner)
                    .with_margin_left(super::app_panel_style::REFRESH_BTN_MARGIN_LEFT)
                    .finish(),
            )
            .finish();

        let search_row_container = Container::new(search_row)
            .with_margin_bottom(super::app_panel_style::SEARCH_BAR_MARGIN_BOTTOM)
            .finish();

        // --- 更新共享 rows（render_fn 闭包会读取这个 Rc） ---
        {
            let new_rows: Vec<ClipboardRecordRow> = filtered
                .iter()
                .map(|r| ClipboardRecordRow {
                    id: r.id,
                    preview: r.preview.clone(),
                    content: r.content.clone(),
                    created_at: r.created_at,
                })
                .collect();
            let mut rows_ref = self.rows.borrow_mut();
            *rows_ref = new_rows;
        }

        // --- 协调 list_state 的 item 数量（add_item / remove） ---
        // 这里借不到 &mut self，因此使用 cell-stored 字段；
        // 协调通过在 self 上加 RefCell<usize> 太重，改为：
        // 我们用 self.rendered_count（usize）跟踪。协调在 View::render 的
        // 不可变借用中做不了，所以 ListState 的 item 数量变更通过
        // TypedActionView 内部或单独的 sync 通道——这里采取简单做法：
        // 在 ListState 创建时按 self.rows 一次性 add 0 个，
        // 之后每次 render 通过 self.rendered_count 字段增量调整。
        // 但 render 是 &self，不可变借用，无法调用 list_state.add_item (需要
        // 内部 RefCell mut borrow)。好在 ListState.add_item 内部已经用
        // RefCell，所以这里 *可以* 在 &self 中调用 add_item。

        let target_count = self.rows.borrow().len();
        let current_count = self.rendered_count.get();
        if target_count > current_count {
            for _ in current_count..target_count {
                self.list_state.add_item();
            }
        } else if target_count < current_count {
            for i in (target_count..current_count).rev() {
                self.list_state.remove(i);
            }
        }
        // 注：上面这两行不修改 self.rendered_count（&self），但实际上
        // list_state 内部已经更新；count 不一致会导致下次 render 多 add。
        // 解决方案：把 rendered_count 改为 Cell<usize> 或把 add/remove 推迟
        // 到 handle_action 里（仅在 selected_record_id 变更或数据变化时同步）。
        // 这里采用 Cell<usize>。
        self.rendered_count.set(target_count);

        // 协调展开行的高度缓存失效（如果被删除/合并）
        if let Some(selected_id) = self.inner.selected_record_id {
            let rows = self.rows.borrow();
            if let Some(idx) = rows.iter().position(|r| r.id == selected_id) {
                self.list_state.invalidate_height_for_index(idx);
            }
        }

        // --- 剪贴板记录列表（List + Scrollable） ---
        let list = List::new(self.list_state.clone()).finish_scrollable();

        let scroll_view = Scrollable::vertical(
            self.scroll_state.clone(),
            list,
            ScrollbarWidth::Auto,
            theme.nonactive_ui_detail().into(),
            theme.active_ui_detail().into(),
            Fill::None,
        )
        .with_overlayed_scrollbar()
        .finish();

        let scrollable: Box<dyn Element> = if self.rows.borrow().is_empty() {
            Shrinkable::new(1.0, Flex::column().finish()).finish()
        } else {
            Shrinkable::new(1.0, scroll_view).finish()
        };

        // --- 全部清空按钮 ---
        let clear_all_button = Align::new(ChildView::new(&self.clear_all_button).finish())
            .finish();

        // --- 内容区布局（用 SavePosition 包裹以记录面板内容原点） ---
        let content_inner = Container::new(
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(search_row_container)
                .with_child(scrollable)
                .with_child(
                    Container::new(clear_all_button)
                        .with_margin_top(super::app_panel_style::CLEAR_BTN_MARGIN_TOP)
                        .finish(),
                )
                .finish(),
        )
        .with_uniform_padding(super::app_panel_style::CONTENT_PADDING)
        .finish();

        let content = SavePosition::new(content_inner, &self.content_position_id).finish();

        let main_layout = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(Shrinkable::new(1., sidebar).finish())
            .with_child(Shrinkable::new(1., content).finish())
            .finish();

        // --- 叠加层：确认清空弹窗 + 右键上下文菜单 ---
        let mut stack = Stack::new();
        stack.add_child(main_layout);

        if self.inner.confirm_clear_shown {
            let dialog = Dialog::new(
                crate::t!("app-panel-confirm-clear-title"),
                Some(crate::t!("app-panel-confirm-clear-desc")),
                dialog_styles(appearance),
            )
            .with_bottom_row_child(ChildView::new(&self.confirm_cancel_button).finish())
            .with_bottom_row_child(
                Container::new(ChildView::new(&self.confirm_delete_button).finish())
                    .with_margin_left(super::app_panel_style::CONFIRM_BTN_MARGIN_LEFT)
                    .finish(),
            )
            .with_width(super::app_panel_style::CONFIRM_DIALOG_WIDTH)
            .build()
            .finish();

            let overlay = Dismiss::new(Align::new(dialog).finish())
                .prevent_interaction_with_other_elements()
                .on_dismiss(|ctx, _| {
                    ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                        ClipboardPageAction::ClearAllCancelled,
                    ));
                })
                .finish();
            stack.add_child(overlay);
        }

        if let Some(menu_state) = &self.context_menu_state {
            // 菜单以 SavePosition 内容区为锚点，offset 已是相对内容区原点的偏移
            let positioning = OffsetPositioning::offset_from_save_position_element(
                self.content_position_id.clone(),
                menu_state.offset,
                PositionedElementOffsetBounds::WindowByPosition,
                PositionedElementAnchor::TopLeft,
                warpui::elements::ChildAnchor::TopLeft,
            );
            stack.add_positioned_overlay_child(
                ChildView::new(&self.context_menu).finish(),
                positioning,
            );
        }

        stack.finish()
    }
}

impl TypedActionView for AppPanelView {
    type Action = AppPanelAction;

    fn handle_action(&mut self, action: &AppPanelAction, ctx: &mut ViewContext<Self>) {
        match action {
            AppPanelAction::SelectSection(section) => {
                self.inner.current_section = *section;
                ctx.notify();
            }
            AppPanelAction::Clipboard(clip_action) => {
                match clip_action {
                    ClipboardPageAction::RecordClicked(_id) => {
                        // 委托给纯状态层：切换展开/收起
                        let model = ClipboardHistoryModel::handle(ctx);
                        model.update(ctx, |model, _ctx| {
                            let _ = self.inner.handle_clipboard_action(clip_action, model);
                        });
                        // 展开状态变化可能改变行高，通知并 invalidate 当前选中行高
                        if let Some(new_id) = self.inner.selected_record_id {
                            let rows = self.rows.borrow();
                            if let Some(idx) = rows.iter().position(|r| r.id == new_id) {
                                self.list_state.invalidate_height_for_index(idx);
                            }
                        }
                        ctx.notify();
                    }
                    ClipboardPageAction::RecordRightClicked { record_id, position } => {
                        // 打开上下文菜单：构造 items 并设置状态
                        let copy_action = AppPanelAction::Clipboard(
                            ClipboardPageAction::ContextMenuCopyRequested(*record_id),
                        );
                        let delete_action = AppPanelAction::Clipboard(
                            ClipboardPageAction::RecordDeleted(*record_id),
                        );
                        let items = vec![
                            MenuItemFields::new(crate::t!("app-panel-context-menu-copy"))
                                .with_on_select_action(copy_action)
                                .into_item(),
                            MenuItemFields::new(crate::t!("app-panel-context-menu-delete"))
                                .with_on_select_action(delete_action)
                                .into_item(),
                        ];
                        self.context_menu.update(ctx, |menu, ctx| {
                            menu.set_items(items, ctx);
                        });
                        self.context_menu_state = Some(ClipboardContextMenuState {
                            record_id: *record_id,
                            offset: *position,
                        });
                        ctx.notify();
                    }
                    ClipboardPageAction::ContextMenuClosed => {
                        self.context_menu_state = None;
                        ctx.notify();
                    }
                    ClipboardPageAction::ContextMenuCopyRequested(record_id) => {
                        let model = ClipboardHistoryModel::handle(ctx);
                        // 1. 在模型上标记刚写入的内容（抑制 watcher 副作用）
                        let content_to_write = model
                            .as_ref(ctx)
                            .records()
                            .iter()
                            .find(|r| r.id == *record_id)
                            .map(|r| r.content.clone());
                        if let Some(content) = content_to_write {
                            model.update(ctx, |model, _ctx| {
                                model.mark_recently_written(content.clone());
                            });
                            // 2. 写入系统剪贴板
                            ctx.clipboard().write(ClipboardContent::plain_text(content.clone()));
                            // 3. 显示 toast
                            let preview = clipboard_history::truncate_chars(&content, 50);
                            let window_id = ctx.window_id();
                            ToastStack::handle(ctx).update(ctx, |stack, ctx| {
                                stack.add_ephemeral_toast(
                                    DismissibleToast::success(crate::t!(
                                        "app-panel-copied-toast",
                                        preview = preview.as_str()
                                    )),
                                    window_id,
                                    ctx,
                                );
                            });
                        }
                        // 4. 关闭菜单
                        self.context_menu_state = None;
                        ctx.notify();
                    }
                    ClipboardPageAction::RecordDeleted(id) => {
                        // 若被删除的正是当前菜单指向的记录，关闭菜单
                        if let Some(state) = &self.context_menu_state {
                            if state.record_id == *id {
                                self.context_menu_state = None;
                            }
                        }
                        let model = ClipboardHistoryModel::handle(ctx);
                        model.update(ctx, |model, _ctx| {
                            let _ = self.inner.handle_clipboard_action(clip_action, model);
                        });
                        ctx.notify();
                    }
                    ClipboardPageAction::ClearAllConfirmed => {
                        self.context_menu_state = None;
                        let model = ClipboardHistoryModel::handle(ctx);
                        model.update(ctx, |model, _ctx| {
                            let _ = self.inner.handle_clipboard_action(clip_action, model);
                        });
                        let window_id = ctx.window_id();
                        ToastStack::handle(ctx).update(ctx, |stack, ctx| {
                            stack.add_ephemeral_toast(
                                DismissibleToast::success(crate::t!("app-panel-cleared-toast")),
                                window_id,
                                ctx,
                            );
                        });
                        ctx.notify();
                    }
                    ClipboardPageAction::SyncRefreshRequested => {
                        let model = ClipboardHistoryModel::handle(ctx);
                        if !model.as_ref(ctx).is_syncing() {
                            model.update(ctx, |model, ctx| {
                                model.trigger_sync_download(ctx);
                            });
                        }
                        ctx.notify();
                    }
                    _ => {
                        let model = ClipboardHistoryModel::handle(ctx);
                        model.update(ctx, |model, _ctx| {
                            let _ = self.inner.handle_clipboard_action(clip_action, model);
                        });
                        ctx.notify();
                    }
                }
            }
        }
    }
}

impl BackingView for AppPanelView {
    type PaneHeaderOverflowMenuAction = AppPanelAction;
    type CustomAction = AppPanelAction;
    type AssociatedData = ();

    fn handle_pane_header_overflow_menu_action(
        &mut self,
        _action: &AppPanelAction,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(PaneEvent::Close);
    }

    fn focus_contents(&mut self, _ctx: &mut ViewContext<Self>) {}

    fn render_header_content(
        &self,
        _ctx: &view::HeaderRenderContext<'_>,
        _app: &AppContext,
    ) -> view::HeaderContent {
        view::HeaderContent::simple(crate::t!("app-panel-title"))
    }

    fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, _ctx: &mut ViewContext<Self>) {
        self.focus_handle = Some(focus_handle);
    }
}

// --- AppPanelPane ---

/// 应用面板 Pane（实现 PaneContent）
pub struct AppPanelPane {
    view: ViewHandle<super::view::PaneView<AppPanelView>>,
    pane_configuration: ModelHandle<PaneConfiguration>,
}

impl AppPanelPane {
    /// Creates an AppPanelPane from an existing AppPanelView handle.
    fn from_view(app_panel_view: ViewHandle<AppPanelView>, ctx: &mut AppContext) -> Self {
        let pane_configuration = app_panel_view.as_ref(ctx).pane_configuration.clone();
        let view = ctx.add_typed_action_view(app_panel_view.window_id(ctx), |ctx| {
            let pane_id = PaneId::from_app_panel_pane_ctx(ctx);
            super::view::PaneView::new(pane_id, app_panel_view, (), pane_configuration.clone(), ctx)
        });

        Self {
            view,
            pane_configuration,
        }
    }

    /// 创建新的应用面板 Pane
    ///
    /// 初始化内部 View 状态、搜索编辑器、确认弹窗按钮、上下文菜单，
    /// 并启动剪贴板监听器。Pane 通过 PaneView 包装后集成到 PaneGroup 系统。
    ///
    /// # 参数
    /// * `section` - 初始显示的子页面
    /// * `_window_id` - 所属窗口 ID（当前未使用）
    /// * `ctx` - 用于创建子 View 和 Model 的上下文
    pub fn new<V: View>(
        section: AppPanelSection,
        _window_id: WindowId,
        ctx: &mut ViewContext<V>,
    ) -> Self {
        let nav_hover_states = AppPanelSection::all()
            .iter()
            .map(|_| MouseStateHandle::default())
            .collect();

        let pane_configuration =
            ctx.add_model(|_ctx| PaneConfiguration::new(crate::t_static!("app-panel-title")));

        let pane_config_clone = pane_configuration.clone();

        // 创建搜索编辑器 + 搜索栏（参照 list_page.rs 模式）
        let search_editor_text = TextOptions::ui_text(None, Appearance::as_ref(ctx));
        let search_editor = ctx.add_typed_action_view(|ctx| {
            EditorView::single_line(SingleLineEditorOptions {
                text: search_editor_text,
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                ..Default::default()
            }, ctx)
        });
        search_editor.update(ctx, |editor, ctx| {
            editor.clear_buffer_and_reset_undo_stack(ctx);
            editor.set_placeholder_text(crate::t!("app-panel-search-placeholder"), ctx);
        });
        let search_bar = ctx.add_typed_action_view(|_| SearchBar::new(search_editor.clone()));

        // 创建确认弹窗按钮
        let confirm_cancel_button = ctx.add_typed_action_view(|_| {
            ActionButton::new(crate::t!("app-panel-cancel"), NakedTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                    ClipboardPageAction::ClearAllCancelled,
                ));
            })
        });
        let confirm_delete_button = ctx.add_typed_action_view(|_| {
            ActionButton::new(crate::t!("app-panel-confirm"), DangerPrimaryTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                    ClipboardPageAction::ClearAllConfirmed,
                ));
            })
        });
        let clear_all_button = ctx.add_typed_action_view(|_| {
            ActionButton::new(crate::t!("app-panel-clear-all"), DangerSecondaryTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                    ClipboardPageAction::ClearAllRequested,
                ));
            })
        });
        let refresh_button = ctx.add_typed_action_view(|_| {
            ActionButton::new(crate::t!("common-refresh"), NakedTheme)
                .with_icon(Icon::RefreshCw04)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                        ClipboardPageAction::SyncRefreshRequested,
                    ));
                })
        });

        // 创建上下文菜单视图（空 items，初始菜单关闭）
        let context_menu = ctx.add_typed_action_view(|_| {
            Menu::new()
                .with_width(super::app_panel_style::CONTEXT_MENU_WIDTH)
                .with_drop_shadow()
                .prevent_interaction_with_other_elements()
        });
        // 订阅菜单关闭事件：Menu 关闭时通知主 view 清空 context_menu_state
        let context_menu_for_close = context_menu.clone();
        let app_panel_view = ctx.add_typed_action_view(|ctx| {
            ctx.subscribe_to_view(&context_menu_for_close, |_, _, event, ctx| {
                if matches!(event, MenuEvent::Close { .. }) {
                    ctx.dispatch_typed_action(&AppPanelAction::Clipboard(
                        ClipboardPageAction::ContextMenuClosed,
                    ));
                }
            });

            // 创建变高 ListState（render_fn 闭包 'static，捕获 weak handle 与 rows Rc）
            let weak_handle: WeakViewHandle<AppPanelView> = ctx.handle();
            let rows: Rc<RefCell<Vec<ClipboardRecordRow>>> =
                Rc::new(RefCell::new(Vec::new()));
            let rows_for_render = rows.clone();
            let content_position_id = format!("app-panel-content-{}", ctx.view_id());
            let content_position_id_for_render = content_position_id.clone();

            let list_state = ListState::new(move |index, _scroll_offset, app| {
                Self::render_clipboard_row(
                    index,
                    &weak_handle,
                    &rows_for_render,
                    &content_position_id_for_render,
                    app,
                )
            });

            // 搜索编辑器订阅：注册在 AppPanelView 自身上下文中，
            // 确保输入变化时 AppPanelView 被标记为脏并重新渲染过滤列表
            let search_editor_for_subscription = search_editor.clone();
            ctx.subscribe_to_view(&search_editor_for_subscription, |_, _, _, ctx| {
                ctx.notify();
            });

            AppPanelView {
                inner: AppPanelViewInner {
                    current_section: section,
                    confirm_clear_shown: false,
                    selected_record_id: None,
                },
                pane_configuration: pane_config_clone,
                focus_handle: None,
                nav_hover_states,
                search_editor,
                search_bar,
                clear_all_button,
                confirm_cancel_button,
                confirm_delete_button,
                refresh_button,
                list_state,
                rendered_count: Cell::new(0),
                rows,
                content_position_id,
                scroll_state: Arc::new(Mutex::new(Default::default())),
                spinner_handle: SpinnerStateHandle::new(),
                context_menu,
                context_menu_state: None,
            }
        });

        // 启动剪贴板轮询
        let model = ClipboardHistoryModel::handle(ctx);
        model.update(ctx, |model, ctx| {
            model.start_watching(ctx);
            model.trigger_sync_download(ctx);
        });

        Self::from_view(app_panel_view, ctx)
    }

    /// 渲染单条剪贴板记录行（List 的 render_fn）
    ///
    /// 通过 weak handle 访问 AppPanelView 的 inner 状态（selected_record_id），
    /// 通过 rows Rc 访问行数据。右击时通过 content_position_id 反查面板内容原点。
    fn render_clipboard_row(
        index: usize,
        weak_handle: &WeakViewHandle<AppPanelView>,
        rows: &Rc<RefCell<Vec<ClipboardRecordRow>>>,
        content_position_id: &str,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let row_data = rows.borrow().get(index).cloned();
        let Some(row) = row_data else {
            // 越界：返回不可见占位（理论上不应到达）
            return ConstrainedBox::new(Rect::new().finish())
                .with_width(1.)
                .with_height(1.)
                .finish();
        };

        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let ff = appearance.ui_font_family();
        let fs = appearance.ui_font_size();
        let dfs = super::app_panel_style::timestamp_font_size(appearance);
        let row_pad = super::app_panel_style::RECORD_ROW_PADDING;
        let cr_small = super::app_panel_style::CORNER_RADIUS_SMALL;
        let del_size = super::app_panel_style::DELETE_ICON_SIZE;
        let row_height = super::app_panel_style::RECORD_ROW_HEIGHT;
        let expanded_height = super::app_panel_style::EXPANDED_ROW_HEIGHT;

        let fg: ColorU = theme.foreground().into_solid();
        let detail_color: ColorU = theme.nonactive_ui_text_color().into_solid();
        let row_hover_bg: Fill = Fill::Solid(theme.nonactive_ui_detail().into_solid());
        let row_no_bg: Fill = Fill::Solid(ColorU::transparent_black());
        let row_selected_bg: Fill = Fill::Solid(theme.surface_2().into_solid());
        let del_hover_bg: Fill = Fill::Solid(theme.nonactive_ui_detail().into_solid());
        let del_no_bg: Fill = Fill::Solid(ColorU::transparent_black());

        // 读取当前展开状态
        let expanded = weak_handle
            .upgrade(app)
            .map(|h| h.as_ref(app).inner.selected_record_id == Some(row.id))
            .unwrap_or(false);

        let row_id = row.id;
        let content_position_id_owned = content_position_id.to_string();

        // 删除按钮（hover 时显示）
        let body_row = Hoverable::new(MouseStateHandle::default(), move |state| {
            let bg = if expanded {
                row_selected_bg
            } else if state.is_hovered() {
                row_hover_bg
            } else {
                row_no_bg
            };

            let delete_btn = if state.is_hovered() {
                let del_id = row_id;
                Some(
                    Hoverable::new(MouseStateHandle::default(), move |del_state| {
                        let del_bg = if del_state.is_hovered() { del_hover_bg } else { del_no_bg };
                        Container::new(
                            ConstrainedBox::new(
                                icons::Icon::X.to_warpui_icon(detail_color.into()).finish(),
                            )
                            .with_width(del_size)
                            .with_height(del_size)
                            .finish(),
                        )
                        .with_background(del_bg)
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(cr_small)))
                        .finish()
                    })
                    .with_cursor(Cursor::PointingHand)
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                            ClipboardPageAction::RecordDeleted(del_id),
                        ));
                    })
                    .finish(),
                )
            } else {
                None
            };

            // 在闭包内创建 body / time，避免克隆 Box<dyn Element>
            let time_str = format_time_i18n(&row.created_at);
            let time = Text::new(time_str, ff, dfs)
                .with_color(detail_color)
                .finish();
            let body_text = if expanded {
                row.content.clone()
            } else {
                row.preview.clone()
            };
            let body = Text::new(body_text, ff, fs).with_color(fg).finish();

            let mut row_content = Flex::row()
                .with_main_axis_alignment(MainAxisAlignment::Start)
                .with_cross_axis_alignment(if expanded {
                    CrossAxisAlignment::Start
                } else {
                    CrossAxisAlignment::Center
                })
                .with_child(
                    Expanded::new(
                        1.,
                        Flex::column()
                            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .with_child(body)
                            .with_child(time)
                            .finish(),
                    )
                    .finish(),
                );

            if let Some(del) = delete_btn {
                row_content = row_content.with_child(del);
            }

            Container::new(row_content.finish())
                .with_uniform_padding(row_pad)
                .with_background(bg)
                .finish()
        });

        let row_element = body_row
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                    ClipboardPageAction::RecordClicked(row_id),
                ));
            })
            .on_right_click(move |ctx, _, position| {
                let Some(bounds) = ctx.element_position_by_id(&content_position_id_owned) else {
                    return;
                };
                let offset = position - bounds.origin();
                ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                    ClipboardPageAction::RecordRightClicked {
                        record_id: row_id,
                        position: offset,
                    },
                ));
            })
            .finish();

        let height = if expanded { expanded_height } else { row_height };
        ConstrainedBox::new(row_element)
            .with_width(f32::INFINITY)
            .with_height(height)
            .finish()
    }
}

impl PaneContent for AppPanelPane {
    fn id(&self) -> PaneId {
        PaneId::from_app_panel_pane_view(&self.view)
    }

    fn attach(
        &self,
        _group: &PaneGroup,
        focus_handle: PaneFocusHandle,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        let pane_id = self.id();
        let pane_group_id = ctx.view_id();
        let window_id = ctx.window_id();
        self.view
            .update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));

        // 注册面板到 Manager
        AppPanelPaneManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.register_pane(self, pane_group_id, window_id, ctx);
        });

        ctx.subscribe_to_view(&self.view, move |group, _, event, ctx| {
            group.handle_pane_view_event(pane_id, event, ctx);
        });
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        detach_type: DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        // 只在关闭时停止监听，移动时保持
        if matches!(detach_type, DetachType::Closed | DetachType::HiddenForClose) {
            let model = ClipboardHistoryModel::handle(ctx);
            model.update(ctx, |model, _ctx| {
                model.stop_watching();
            });

            // 从 Manager 注销面板
            let pane_group_id = ctx.view_id();
            let window_id = ctx.window_id();
            AppPanelPaneManager::handle(ctx).update(ctx, |manager, ctx| {
                manager.deregister_pane(&window_id, pane_group_id, self.id(), ctx);
            });
        }
        ctx.unsubscribe_to_view(&self.view);
    }

    fn snapshot(&self, app: &AppContext) -> LeafContents {
        let section = self.view.as_ref(app).child(app).as_ref(app).inner.current_section;
        LeafContents::AppPanel(AppPanelPaneSnapshot {
            current_section: section,
        })
    }

    fn has_application_focus(&self, ctx: &mut ViewContext<PaneGroup>) -> bool {
        self.view.is_self_or_child_focused(ctx)
    }

    fn focus(&self, _ctx: &mut ViewContext<PaneGroup>) {}

    fn shareable_link(
        &self,
        _ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<ShareableLink, ShareableLinkError> {
        Ok(ShareableLink::Base)
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, ctx: &AppContext) -> bool {
        self.view.as_ref(ctx).is_being_dragged()
    }
}

// --- PaneId helpers ---

impl PaneId {
    /// Creates a PaneId for the app panel from a view context.
    fn from_app_panel_pane_ctx(ctx: &ViewContext<super::view::PaneView<AppPanelView>>) -> Self {
        Self::new_from_ctx(IPaneType::AppPanel, ctx)
    }

    /// Creates a PaneId for the app panel from a view handle.
    fn from_app_panel_pane_view(view: &ViewHandle<super::view::PaneView<AppPanelView>>) -> Self {
        Self::new(IPaneType::AppPanel, view)
    }
}

/// 格式化时间为国际化的显示字符串
fn format_time_i18n(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);
    if diff.num_seconds() < 60 {
        crate::t!("app-panel-time-just-now")
    } else if diff.num_minutes() < 60 {
        crate::t!("app-panel-time-minutes-ago", count = diff.num_minutes())
    } else if diff.num_hours() < 24 {
        crate::t!("app-panel-time-hours-ago", count = diff.num_hours())
    } else if diff.num_days() < 7 {
        crate::t!("app-panel-time-days-ago", count = diff.num_days())
    } else {
        dt.format("%m-%d %H:%M").to_string()
    }
}
