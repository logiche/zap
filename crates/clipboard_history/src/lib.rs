//! 剪贴板历史管理核心逻辑
//!
//! 提供 SQLite 持久化、内存缓存、搜索与增删操作。
//!
//! author logic
//! date 2026-05-31

pub mod db;
pub mod model;
pub mod record;
pub mod watcher;

pub use model::{ClipboardHistoryModel, ClipboardHistoryModelEvent};
pub use record::{make_preview, truncate_chars, ClipboardRecord};
pub use watcher::ClipboardWatcher;
