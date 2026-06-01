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
use warpui::clipboard::ClipboardContent;
use warpui::elements::{
    Align, Border, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    Dismiss, Element, Expanded, Fill, Flex, Hoverable, MainAxisAlignment,
    MainAxisSize, MouseStateHandle, ParentElement, Radius, Shrinkable, Stack, Text,
};
use warpui::platform::Cursor;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Entity, ModelHandle, SingletonEntity as _, TypedActionView,
    View, ViewContext, ViewHandle, WindowId,
};

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
        let ui_font_family = appearance.ui_font_family();
        let ui_font_size = appearance.ui_font_size();

        // 从搜索编辑器读取实时文本
        let search_term = self.search_editor.as_ref(ctx).buffer_text(ctx);
        let model = ClipboardHistoryModel::handle(ctx);
        let records = model.as_ref(ctx).records();
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

        // --- 搜索框 ---
        let search_bar_element = Container::new(ChildView::new(&self.search_bar).finish())
            .with_margin_bottom(super::app_panel_style::SEARCH_BAR_MARGIN_BOTTOM)
            .finish();

        // --- 剪贴板记录列表 ---
        let fg: ColorU = theme.foreground().into_solid();
        let detail: ColorU = theme.nonactive_ui_text_color().into_solid();
        let row_hover_bg: Fill = Fill::Solid(theme.nonactive_ui_detail().into_solid());
        let row_no_bg: Fill = Fill::Solid(ColorU::transparent_black());
        let del_hover_bg: Fill = Fill::Solid(theme.nonactive_ui_detail().into_solid());
        let del_no_bg: Fill = Fill::Solid(ColorU::transparent_black());

        let mut records_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min);

        for record in &filtered {
            let preview_str = record.preview.clone();
            let time_str = format_time_i18n(&record.created_at);
            let record_id = record.id;
            let font_family = ui_font_family;
            let font_size = ui_font_size;

            let row = Hoverable::new(MouseStateHandle::default(), move |state| {
                let bg = if state.is_hovered() { row_hover_bg } else { row_no_bg };

                let preview = Text::new(preview_str.clone(), font_family, font_size)
                    .with_color(fg)
                    .finish();
                let time = Text::new(time_str.clone(), font_family, super::app_panel_style::timestamp_font_size(appearance))
                    .with_color(detail)
                    .finish();

                // 删除按钮（hover 时显示）
                let delete_btn = if state.is_hovered() {
                    let del_id = record_id;
                    Some(
                        Hoverable::new(MouseStateHandle::default(), move |del_state| {
                            let del_bg = if del_state.is_hovered() { del_hover_bg } else { del_no_bg };
                            Container::new(
                                ConstrainedBox::new(
                                    icons::Icon::X.to_warpui_icon(detail.into()).finish()
                                ).with_width(super::app_panel_style::DELETE_ICON_SIZE).with_height(super::app_panel_style::DELETE_ICON_SIZE).finish()
                            )
                            .with_background(del_bg)
                            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(super::app_panel_style::CORNER_RADIUS_SMALL)))
                            .finish()
                        })
                        .with_cursor(Cursor::PointingHand)
                        .on_click(move |ctx, _, _| {
                            ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                                ClipboardPageAction::RecordDeleted(del_id),
                            ));
                        })
                        .finish()
                    )
                } else {
                    None
                };

                let mut row_content = Flex::row()
                    .with_main_axis_alignment(MainAxisAlignment::Start)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(
                        Expanded::new(1., Flex::column()
                            .with_child(preview)
                            .with_child(time)
                            .finish()
                        ).finish()
                    );

                if let Some(del) = delete_btn {
                    row_content = row_content.with_child(del);
                }

                Container::new(row_content.finish())
                    .with_uniform_padding(super::app_panel_style::RECORD_ROW_PADDING)
                    .with_background(bg)
                    .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                    ClipboardPageAction::RecordClicked(record_id),
                ));
            })
            .finish();

            records_column = records_column.with_child(row);
        }

        // --- 全部清空按钮 ---
        let clear_all_button = Align::new(ChildView::new(&self.clear_all_button).finish())
            .finish();

        // --- 内容区布局 ---
        let content = Container::new(
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(search_bar_element)
                .with_child(records_column.finish())
                .with_child(
                    Container::new(clear_all_button)
                        .with_margin_top(super::app_panel_style::CLEAR_BTN_MARGIN_TOP)
                        .finish(),
                )
                .finish(),
        )
        .with_uniform_padding(super::app_panel_style::CONTENT_PADDING)
        .finish();

        let main_layout = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(Shrinkable::new(1., sidebar).finish())
            .with_child(Shrinkable::new(1., content).finish())
            .finish();

        // --- 确认清空弹窗 ---
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

            let overlay = Dismiss::new(
                Align::new(dialog).finish()
            )
                .prevent_interaction_with_other_elements()
                .on_dismiss(|ctx, _| {
                    ctx.dispatch_typed_action(AppPanelAction::Clipboard(
                        ClipboardPageAction::ClearAllCancelled,
                    ));
                })
                .finish();

            let mut stack = Stack::new();
            stack.add_child(main_layout);
            stack.add_child(overlay);
            stack.finish()
        } else {
            main_layout
        }
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
                    ClipboardPageAction::RecordClicked(id) => {
                        let model = ClipboardHistoryModel::handle(ctx);
                        if let Some(record) = model.as_ref(ctx).records().iter().find(|r| r.id == *id) {
                            let content = record.content.clone();
                            // 写入系统剪贴板
                            ctx.clipboard().write(ClipboardContent::plain_text(content.clone()));
                            // 显示 toast
                            let preview = clipboard_history::truncate_chars(&content, 50);
                            let window_id = ctx.window_id();
                            ToastStack::handle(ctx).update(ctx, |stack, ctx| {
                                stack.add_ephemeral_toast(
                                    DismissibleToast::success(crate::t!("app-panel-copied-toast", preview = preview.as_str())),
                                    window_id, ctx,
                                );
                            });
                        }
                        ctx.notify();
                    }
                    ClipboardPageAction::ClearAllConfirmed => {
                        let model = ClipboardHistoryModel::handle(ctx);
                        model.update(ctx, |model, _ctx| {
                            let _ = self.inner.handle_clipboard_action(clip_action, model);
                        });
                        let window_id = ctx.window_id();
                        ToastStack::handle(ctx).update(ctx, |stack, ctx| {
                            stack.add_ephemeral_toast(
                                DismissibleToast::success(crate::t!("app-panel-cleared-toast")),
                                window_id, ctx,
                            );
                        });
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
    /// 初始化内部 View 状态、搜索编辑器、确认弹窗按钮，
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
        ctx.subscribe_to_view(&search_editor, |_, _, _, ctx| {
            ctx.notify();
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

        let app_panel_view = ctx.add_typed_action_view(|_ctx| {
            AppPanelView {
                inner: AppPanelViewInner {
                    current_section: section,
                    confirm_clear_shown: false,
                },
                pane_configuration: pane_config_clone,
                focus_handle: None,
                nav_hover_states,
                search_editor,
                search_bar,
                clear_all_button,
                confirm_cancel_button,
                confirm_delete_button,
            }
        });

        // 启动剪贴板轮询
        let model = ClipboardHistoryModel::handle(ctx);
        model.update(ctx, |model, ctx| {
            model.start_watching(ctx);
        });

        Self::from_view(app_panel_view, ctx)
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
