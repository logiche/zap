//! 应用面板 Pane 管理器
//!
//! 每个窗口最多只有一个应用面板 Pane，通过 Manager 单例跟踪。
//! 打开时先查找已有面板，存在则聚焦而非重复创建。
//!
//! author logic
//! date 2026-05-31

use std::collections::HashMap;

use warpui::{Entity, EntityId, ModelContext, SingletonEntity, WindowId};

use crate::pane_group::PaneId;
use crate::PaneViewLocator;

use super::app_panel_pane::AppPanelPane;
use super::PaneContent;

/// 应用面板 Pane 数据
struct AppPanelPaneData {
    /// 面板在 PaneGroup 中的定位信息
    locator: Option<PaneViewLocator>,
}

/// 应用面板 Pane 管理器（全局单例）
///
/// 维护每个窗口的应用面板状态，确保每个窗口最多只有一个应用面板。
#[derive(Default)]
pub struct AppPanelPaneManager {
    panes: HashMap<WindowId, AppPanelPaneData>,
}

impl AppPanelPaneManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self::default()
    }

    /// 查找指定窗口中已打开的应用面板
    pub fn find_pane(&self, window_id: WindowId) -> Option<PaneViewLocator> {
        self.panes.get(&window_id).and_then(|data| data.locator)
    }

    /// 注册应用面板（在 attach 时调用）
    pub fn register_pane(
        &mut self,
        pane: &AppPanelPane,
        pane_group_id: EntityId,
        window_id: WindowId,
        _ctx: &mut ModelContext<Self>,
    ) {
        self.register_pane_inner(pane.id(), pane_group_id, window_id);
    }

    /// 注销应用面板（在 detach 时调用）
    pub fn deregister_pane(
        &mut self,
        window_id: &WindowId,
        pane_group_id: EntityId,
        pane_id: PaneId,
        _ctx: &mut ModelContext<Self>,
    ) {
        self.deregister_pane_inner(*window_id, pane_group_id, pane_id);
    }

    /// 注册面板的核心逻辑
    fn register_pane_inner(
        &mut self,
        pane_id: PaneId,
        pane_group_id: EntityId,
        window_id: WindowId,
    ) {
        self.panes
            .entry(window_id)
            .or_insert_with(|| AppPanelPaneData { locator: None })
            .locator = Some(PaneViewLocator {
                pane_group_id,
                pane_id,
            });
    }

    /// 注销面板的核心逻辑
    fn deregister_pane_inner(
        &mut self,
        window_id: WindowId,
        pane_group_id: EntityId,
        pane_id: PaneId,
    ) {
        if let Some(data) = self.panes.get_mut(&window_id) {
            let locator = PaneViewLocator {
                pane_group_id,
                pane_id,
            };
            if data.locator == Some(locator) {
                data.locator = None;
            }
        }
    }
}

impl Entity for AppPanelPaneManager {
    type Event = ();
}

/// 标记为全局单例
impl SingletonEntity for AppPanelPaneManager {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane_group::pane::PaneId;

    fn test_manager() -> AppPanelPaneManager {
        AppPanelPaneManager::new()
    }

    fn test_locator(id: usize) -> PaneViewLocator {
        PaneViewLocator {
            pane_group_id: EntityId::from_usize(id),
            pane_id: PaneId::dummy_pane_id(),
        }
    }

    // --- new ---

    #[test]
    fn new_创建空管理器() {
        let manager = test_manager();
        assert!(manager.find_pane(WindowId::from_usize(1)).is_none());
    }

    // --- find_pane ---

    #[test]
    fn find_pane_未注册窗口返回none() {
        let manager = test_manager();
        assert!(manager.find_pane(WindowId::from_usize(0)).is_none());
    }

    #[test]
    fn find_pane_注册后能查找到() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);
        let locator = test_locator(10);

        manager.register_pane_inner(locator.pane_id, locator.pane_group_id, window_id);

        let found = manager.find_pane(window_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), locator);
    }

    // --- register_pane_inner ---

    #[test]
    fn register_首次注册成功() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);
        let locator = test_locator(1);

        manager.register_pane_inner(locator.pane_id, locator.pane_group_id, window_id);

        assert_eq!(manager.find_pane(window_id), Some(locator));
    }

    #[test]
    fn register_重复注册覆盖旧值() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);

        let locator1 = test_locator(1);
        let locator2 = test_locator(2);

        manager.register_pane_inner(locator1.pane_id, locator1.pane_group_id, window_id);
        manager.register_pane_inner(locator2.pane_id, locator2.pane_group_id, window_id);

        // 第二次注册覆盖第一次
        assert_eq!(manager.find_pane(window_id), Some(locator2));
    }

    #[test]
    fn register_不同窗口各自独立() {
        let mut manager = test_manager();
        let win1 = WindowId::from_usize(1);
        let win2 = WindowId::from_usize(2);
        let locator1 = test_locator(1);
        let locator2 = test_locator(2);

        manager.register_pane_inner(locator1.pane_id, locator1.pane_group_id, win1);
        manager.register_pane_inner(locator2.pane_id, locator2.pane_group_id, win2);

        assert_eq!(manager.find_pane(win1), Some(locator1));
        assert_eq!(manager.find_pane(win2), Some(locator2));
    }

    // --- deregister_pane_inner ---

    #[test]
    fn deregister_已注册面板注销成功() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);
        let locator = test_locator(1);

        manager.register_pane_inner(locator.pane_id, locator.pane_group_id, window_id);
        manager.deregister_pane_inner(window_id, locator.pane_group_id, locator.pane_id);

        assert!(manager.find_pane(window_id).is_none());
    }

    #[test]
    fn deregister_不匹配的locator不清除() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);
        let locator = test_locator(1);
        let wrong_group_id = EntityId::from_usize(999);

        manager.register_pane_inner(locator.pane_id, locator.pane_group_id, window_id);
        // 用不匹配的 pane_group_id 注销
        manager.deregister_pane_inner(window_id, wrong_group_id, locator.pane_id);

        // locator 仍然存在
        assert_eq!(manager.find_pane(window_id), Some(locator));
    }

    #[test]
    fn deregister_未注册窗口安全处理() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(99);

        // 不应 panic
        manager.deregister_pane_inner(window_id, EntityId::from_usize(1), PaneId::dummy_pane_id());
    }

    // --- 完整流程：模拟工具栏"应用"按钮行为 ---

    #[test]
    fn 完整流程_点击应用按钮打开面板() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);

        // 1. 初始状态：未找到面板
        assert!(manager.find_pane(window_id).is_none());

        // 2. 点击"应用"按钮 → 创建面板并注册
        let locator = test_locator(1);
        manager.register_pane_inner(locator.pane_id, locator.pane_group_id, window_id);

        // 3. 再次查找：找到已注册的面板
        let found = manager.find_pane(window_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), locator);
    }

    #[test]
    fn 完整流程_关闭面板后可重新打开() {
        let mut manager = test_manager();
        let window_id = WindowId::from_usize(1);

        // 1. 打开面板
        let locator = test_locator(1);
        manager.register_pane_inner(locator.pane_id, locator.pane_group_id, window_id);
        assert!(manager.find_pane(window_id).is_some());

        // 2. 关闭面板
        manager.deregister_pane_inner(window_id, locator.pane_group_id, locator.pane_id);
        assert!(manager.find_pane(window_id).is_none());

        // 3. 重新打开面板
        let new_locator = test_locator(2);
        manager.register_pane_inner(new_locator.pane_id, new_locator.pane_group_id, window_id);
        assert!(manager.find_pane(window_id).is_some());
        assert_eq!(manager.find_pane(window_id), Some(new_locator));
    }

    #[test]
    fn 完整流程_多窗口独立管理() {
        let mut manager = test_manager();
        let win1 = WindowId::from_usize(1);
        let win2 = WindowId::from_usize(2);

        // 窗口1 打开面板
        let loc1 = test_locator(1);
        manager.register_pane_inner(loc1.pane_id, loc1.pane_group_id, win1);

        // 窗口2 打开面板
        let loc2 = test_locator(2);
        manager.register_pane_inner(loc2.pane_id, loc2.pane_group_id, win2);

        // 关闭窗口1 的面板
        manager.deregister_pane_inner(win1, loc1.pane_group_id, loc1.pane_id);

        // 窗口1 已关闭，窗口2 仍然存在
        assert!(manager.find_pane(win1).is_none());
        assert_eq!(manager.find_pane(win2), Some(loc2));
    }
}
