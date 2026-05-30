//! 应用面板核心 View
//!
//! 管理侧边导航 + 内容区布局的状态。
//! 实际的 View trait 实现由 app/ 中的胶水层完成。
//!
//! author logic
//! date 2026-05-31

use clipboard_history::{ClipboardHistoryModel, ClipboardRecord};
use warpui::Entity;

use crate::clipboard_page::ClipboardPageAction;
use crate::nav::AppPanelSection;

/// 应用面板 View 的外部事件（供胶水层订阅）
#[derive(Clone, Debug)]
pub enum AppPanelViewInnerEvent {
    /// 请求关闭面板
    Close,
    /// 请求显示 toast
    ShowToast { message: String },
}

/// 应用面板核心状态
pub struct AppPanelViewInner {
    /// 当前子页面
    pub current_section: AppPanelSection,
    /// 搜索关键词
    pub search_query: String,
    /// 是否显示清空确认弹窗
    pub confirm_clear_shown: bool,
}

impl Entity for AppPanelViewInner {
    type Event = AppPanelViewInnerEvent;
}

impl AppPanelViewInner {
    /// 创建新的 View 状态
    pub fn new() -> Self {
        Self {
            current_section: AppPanelSection::default(),
            search_query: String::new(),
            confirm_clear_shown: false,
        }
    }

    /// 获取当前过滤后的剪贴板记录
    pub fn filtered_records<'a>(
        &self,
        records: &'a [ClipboardRecord],
    ) -> Vec<&'a ClipboardRecord> {
        if self.search_query.is_empty() {
            records.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            records
                .iter()
                .filter(|r| r.content.to_lowercase().contains(&query))
                .collect()
        }
    }

    /// 处理剪贴板页面 Action
    pub fn handle_clipboard_action(
        &mut self,
        action: &ClipboardPageAction,
        model: &mut ClipboardHistoryModel,
    ) -> Vec<AppPanelViewInnerEvent> {
        let mut events = Vec::new();
        match action {
            ClipboardPageAction::SearchQueryChanged(query) => {
                self.search_query = query.clone();
            }
            ClipboardPageAction::RecordClicked(id) => {
                if let Some(record) = model.records().iter().find(|r| r.id == *id) {
                    let content = record.content.clone();
                    events.push(AppPanelViewInnerEvent::ShowToast {
                        message: format!("已复制: {}", &content[..content.len().min(50)]),
                    });
                }
            }
            ClipboardPageAction::RecordDeleted(id) => {
                let _ = model.delete(*id);
            }
            ClipboardPageAction::ClearAllRequested => {
                self.confirm_clear_shown = true;
            }
            ClipboardPageAction::ClearAllConfirmed => {
                let _ = model.clear_all();
                self.confirm_clear_shown = false;
                events.push(AppPanelViewInnerEvent::ShowToast {
                    message: "已清空全部剪贴板历史".to_string(),
                });
            }
            ClipboardPageAction::ClearAllCancelled => {
                self.confirm_clear_shown = false;
            }
        }
        events
    }
}
