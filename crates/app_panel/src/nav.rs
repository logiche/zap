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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_为clipboard() {
        assert_eq!(AppPanelSection::default(), AppPanelSection::Clipboard);
    }

    #[test]
    fn label_返回正确名称() {
        assert_eq!(AppPanelSection::Clipboard.label(), "剪贴板");
    }

    #[test]
    fn all_包含clipboard() {
        let all = AppPanelSection::all();
        assert_eq!(all.len(), 1);
        assert!(all.contains(&AppPanelSection::Clipboard));
    }
}
