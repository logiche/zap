//! 应用面板侧边导航枚举
//!
//! author logic
//! date 2026-05-31

/// 应用面板子页面
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum AppPanelSection {
    /// 剪贴板历史
    #[default]
    Clipboard,
}

impl AppPanelSection {
    /// 返回侧边栏显示名称
    pub fn label(&self) -> &'static str {
        match self {
            AppPanelSection::Clipboard => "剪贴板",
        }
    }

    /// 返回全部枚举变体
    pub fn all() -> Vec<Self> {
        vec![AppPanelSection::Clipboard]
    }
}
