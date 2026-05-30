//! 应用面板 Pane 胶水层
//!
//! 桥接 app_panel crate 的 AppPanelViewInner 与 app/ 内部的 PaneContent/BackingView trait。
//!
//! author logic
//! date 2026-05-31

use app_panel::clipboard_page::ClipboardPageAction;
use app_panel::nav::AppPanelSection;
use app_panel::{AppPanelViewInner, AppPanelViewInnerEvent};
use clipboard_history::ClipboardHistoryModel;
use pathfinder_color::ColorU;
use settings::Setting;
use warpui::elements::{
    Border, ConstrainedBox, Container, CrossAxisAlignment,
    Element, Fill, Flex, Hoverable, MainAxisAlignment,
    MainAxisSize, MouseStateHandle, ParentElement, Shrinkable, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::platform::Cursor;
use warpui::{
    AppContext, Entity, ModelHandle, SingletonEntity, SingletonEntity as _, TypedActionView,
    View, ViewContext, ViewHandle, WindowId,
};

use crate::app_state::{AppPanelPaneSnapshot, LeafContents};
use crate::appearance::Appearance;
use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::pane::IPaneType;
use crate::pane_group::pane::view;

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

        // 获取记录
        let model = ClipboardHistoryModel::handle(ctx);
        let records = model.as_ref(ctx).records();
        let filtered = self.inner.filtered_records(records);

        // 侧边导航
        let mut nav_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min);

        for (idx, section) in AppPanelSection::all().iter().enumerate() {
            let hover = self.nav_hover_states[idx].clone();
            let is_active = *section == self.inner.current_section;
            let label = section.label();
            let section = *section;
            let font_family = ui_font_family;
            let font_size = ui_font_size;
            let fg: pathfinder_color::ColorU = theme.foreground().into_solid();
            let active_bg: Fill = Fill::Solid(theme.active_ui_detail().into_solid());
            let hover_bg: Fill = Fill::Solid(theme.nonactive_ui_detail().into_solid());
            let no_bg: Fill = Fill::Solid(ColorU::transparent_black());

            let item = Hoverable::new(hover, move |state| {
                let bg = if is_active {
                    active_bg
                } else if state.is_hovered() {
                    hover_bg
                } else {
                    no_bg
                };

                let text = Text::new(label, font_family, font_size)
                    .with_style(Properties::default().weight(
                        if is_active { Weight::Semibold } else { Weight::Normal },
                    ))
                    .with_color(fg)
                    .finish();

                Container::new(text)
                    .with_padding_left(16.)
                    .with_background(bg)
                    .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(AppPanelAction::SelectSection(section));
            })
            .finish();

            nav_column = nav_column.with_child(item);
        }

        let sidebar = ConstrainedBox::new(
            Container::new(nav_column.finish())
                .with_border(Border::right(1.).with_border_fill(theme.outline()))
                .with_uniform_padding(8.)
                .finish(),
        )
        .with_width(200.)
        .finish();

        // 剪贴板记录列表
        let fg: pathfinder_color::ColorU = theme.foreground().into_solid();
        let detail: pathfinder_color::ColorU = theme.nonactive_ui_text_color().into_solid();
        let hover_bg: Fill = Fill::Solid(theme.nonactive_ui_detail().into_solid());
        let no_bg: Fill = Fill::Solid(ColorU::transparent_black());

        let mut records_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min);

        for record in &filtered {
            let preview_str = record.preview.clone();
            let time_str = app_panel::clipboard_page::format_time(&record.created_at);
            let record_id = record.id;
            let font_family = ui_font_family;
            let font_size = ui_font_size;

            let row = Hoverable::new(MouseStateHandle::default(), move |state| {
                let bg = if state.is_hovered() { hover_bg } else { no_bg };

                let preview = Text::new(preview_str.clone(), font_family, font_size)
                    .with_color(fg)
                    .finish();
                let time = Text::new(time_str.clone(), font_family, 11.)
                    .with_color(detail)
                    .finish();

                Container::new(
                    Flex::row()
                        .with_main_axis_alignment(MainAxisAlignment::Start)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(preview)
                        .with_child(time)
                        .finish(),
                )
                .with_uniform_padding(8.)
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

        let content = Container::new(records_column.finish())
            .with_uniform_padding(16.)
            .finish();

        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(Shrinkable::new(1., sidebar).finish())
            .with_child(Shrinkable::new(1., content).finish())
            .finish()
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
                let model = ClipboardHistoryModel::handle(ctx);
                model.update(ctx, |model, _ctx| {
                    let _events = self.inner.handle_clipboard_action(clip_action, model);
                });
                ctx.notify();
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
        view::HeaderContent::simple("应用")
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
            ctx.add_model(|_ctx| PaneConfiguration::new("应用"));

        let pane_config_clone = pane_configuration.clone();
        let app_panel_view = ctx.add_typed_action_view(|ctx| {
            AppPanelView {
                inner: AppPanelViewInner {
                    current_section: section,
                    search_query: String::new(),
                    confirm_clear_shown: false,
                },
                pane_configuration: pane_config_clone,
                focus_handle: None,
                nav_hover_states,
            }
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
        self.view
            .update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));

        ctx.subscribe_to_view(&self.view, move |group, _, event, ctx| {
            group.handle_pane_view_event(pane_id, event, ctx);
        });
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        _detach_type: DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
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
    fn from_app_panel_pane_ctx(ctx: &ViewContext<super::view::PaneView<AppPanelView>>) -> Self {
        Self::new_from_ctx(IPaneType::AppPanel, ctx)
    }

    fn from_app_panel_pane_view(view: &ViewHandle<super::view::PaneView<AppPanelView>>) -> Self {
        Self::new(IPaneType::AppPanel, view)
    }
}
