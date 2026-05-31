//! 剪贴板历史 Model（单例）
//!
//! 提供内存缓存 + DB 持久化的统一接口。
//!
//! author logic
//! date 2026-05-31

use std::path::Path;

use warpui::{Entity, ModelContext, SingletonEntity};

use crate::db::ClipboardDb;
use crate::record::ClipboardRecord;
use crate::watcher::ClipboardWatcher;

/// 默认最大保留条数
const DEFAULT_MAX_RECORDS: usize = 500;

/// 剪贴板历史单例 Model
pub struct ClipboardHistoryModel {
    db: ClipboardDb,
    records: Vec<ClipboardRecord>,
    max_records: usize,
    watcher: Option<ClipboardWatcher>,
}

impl Entity for ClipboardHistoryModel {
    type Event = ClipboardHistoryModelEvent;
}

/// Model 变更事件
#[derive(Clone, Debug)]
pub enum ClipboardHistoryModelEvent {
    /// 新增了一条记录
    RecordAdded(ClipboardRecord),
    /// 删除了一条记录
    RecordDeleted(i64),
    /// 全部清空
    AllCleared,
    /// 批量淘汰旧记录
    RecordsEvicted(Vec<i64>),
}

impl ClipboardHistoryModel {
    /// 创建 Model 并打开数据库
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        let mut db = ClipboardDb::open(db_path)?;
        let records = db.load_all()?;
        Ok(Self {
            db,
            records,
            max_records: DEFAULT_MAX_RECORDS,
            watcher: None,
        })
    }

    /// 创建内存数据库的 Model（用于测试）
    pub fn new_in_memory() -> anyhow::Result<Self> {
        let mut db = ClipboardDb::open_in_memory()?;
        let records = db.load_all()?;
        Ok(Self {
            db,
            records,
            max_records: DEFAULT_MAX_RECORDS,
            watcher: None,
        })
    }

    /// 获取全部记录（按时间倒序）
    pub fn records(&self) -> &[ClipboardRecord] {
        &self.records
    }

    /// 搜索记录（子串匹配）
    pub fn search(&self, query: &str) -> Vec<&ClipboardRecord> {
        let query_lower = query.to_lowercase();
        self.records
            .iter()
            .filter(|r| r.content.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// 新增一条记录
    pub fn add_record(
        &mut self,
        content: String,
    ) -> anyhow::Result<Option<ClipboardHistoryModelEvent>> {
        if content.is_empty() {
            return Ok(None);
        }
        // 去重：与最近一条相同则跳过
        if self.records.first().map(|r| r.content.as_str()) == Some(content.as_str()) {
            return Ok(None);
        }
        let record = self.db.insert(&content)?;
        self.records.insert(0, record.clone());

        // 淘汰超出上限的旧记录
        let evicted = self.db.evict_oldest(self.max_records)?;
        if !evicted.is_empty() {
            self.records.retain(|r| !evicted.contains(&r.id));
        }

        Ok(Some(ClipboardHistoryModelEvent::RecordAdded(record)))
    }

    /// 删除单条记录
    pub fn delete(&mut self, id: i64) -> anyhow::Result<Option<ClipboardHistoryModelEvent>> {
        if self.db.delete(id)? {
            self.records.retain(|r| r.id != id);
            Ok(Some(ClipboardHistoryModelEvent::RecordDeleted(id)))
        } else {
            Ok(None)
        }
    }

    /// 清空全部记录
    pub fn clear_all(&mut self) -> anyhow::Result<ClipboardHistoryModelEvent> {
        self.db.clear_all()?;
        self.records.clear();
        Ok(ClipboardHistoryModelEvent::AllCleared)
    }

    /// 启动剪贴板监听（事件驱动）
    ///
    /// 后台线程通过 Windows 原生 API 监听剪贴板变化，
    /// 仅在内容实际改变时读取并通知。
    pub fn start_watching(&mut self, ctx: &mut ModelContext<Self>) {
        if self.watcher.is_some() {
            return;
        }

        match ClipboardWatcher::start() {
            Ok(watcher) => {
                let rx = watcher.receiver();
                self.watcher = Some(watcher);
                let _ = ctx.spawn_stream_local(rx, Self::on_clipboard_content, |_, _| {});
            }
            Err(e) => {
                log::error!("Failed to start clipboard watcher: {e}");
            }
        }
    }

    /// 停止剪贴板监听
    pub fn stop_watching(&mut self) {
        if let Some(mut watcher) = self.watcher.take() {
            watcher.stop();
        }
    }

    /// 处理从后台线程收到的剪贴板内容
    fn on_clipboard_content(&mut self, content: String, ctx: &mut ModelContext<Self>) {
        let _ = self.add_record(content);
        ctx.notify();
    }
}

impl SingletonEntity for ClipboardHistoryModel {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model() -> ClipboardHistoryModel {
        ClipboardHistoryModel::new_in_memory().expect("failed to create in-memory model")
    }

    // --- add_record ---

    #[test]
    fn add_record_有效内容返回record_added() {
        let mut model = test_model();

        let event = model.add_record("hello".to_string()).expect("add_record failed");

        assert!(event.is_some());
        let ClipboardHistoryModelEvent::RecordAdded(record) = event.unwrap() else {
            panic!("expected RecordAdded");
        };
        assert_eq!(record.content, "hello");
        assert_eq!(model.records().len(), 1);
    }

    #[test]
    fn add_record_空字符串返回none() {
        let mut model = test_model();

        let event = model.add_record(String::new()).expect("add_record failed");
        assert!(event.is_none());
        assert!(model.records().is_empty());
    }

    #[test]
    fn add_record_重复内容返回none() {
        let mut model = test_model();

        let e1 = model.add_record("dup".to_string()).expect("add failed");
        assert!(e1.is_some());

        let e2 = model.add_record("dup".to_string()).expect("add failed");
        assert!(e2.is_none());
        assert_eq!(model.records().len(), 1);
    }

    #[test]
    fn add_record_不同内容依次添加() {
        let mut model = test_model();

        model.add_record("first".to_string()).expect("add failed");
        model.add_record("second".to_string()).expect("add failed");
        model.add_record("third".to_string()).expect("add failed");

        assert_eq!(model.records().len(), 3);
        // 按时间倒序：最新的在前
        assert_eq!(model.records()[0].content, "third");
        assert_eq!(model.records()[2].content, "first");
    }

    #[test]
    fn add_record_重复但非最近一条仍添加() {
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");
        // "a" 不再是最近一条，应该可以再次添加
        let event = model.add_record("a".to_string()).expect("add failed");
        assert!(event.is_some());
        assert_eq!(model.records().len(), 3);
    }

    // --- delete ---

    #[test]
    fn delete_存在的id返回record_deleted() {
        let mut model = test_model();

        let event = model.add_record("to delete".to_string())
            .expect("add failed")
            .expect("should have event");
        let ClipboardHistoryModelEvent::RecordAdded(record) = event else {
            panic!("expected RecordAdded");
        };

        let del_event = model.delete(record.id).expect("delete failed");
        assert!(del_event.is_some());
        assert!(model.records().is_empty());
    }

    #[test]
    fn delete_不存在的id返回none() {
        let mut model = test_model();

        let event = model.delete(99999).expect("delete failed");
        assert!(event.is_none());
    }

    // --- clear_all ---

    #[test]
    fn clear_all_清空所有记录() {
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");

        let event = model.clear_all().expect("clear_all failed");
        assert!(matches!(event, ClipboardHistoryModelEvent::AllCleared));
        assert!(model.records().is_empty());
    }

    // --- search ---

    #[test]
    fn search_子串匹配() {
        let mut model = test_model();

        model.add_record("Hello World".to_string()).expect("add failed");
        model.add_record("Rust Programming".to_string()).expect("add failed");
        model.add_record("hello rust".to_string()).expect("add failed");

        let results = model.search("hello");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_大小写不敏感() {
        let mut model = test_model();

        model.add_record("Hello World".to_string()).expect("add failed");

        let results = model.search("HELLO");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_无匹配返回空() {
        let mut model = test_model();

        model.add_record("hello".to_string()).expect("add failed");

        let results = model.search("xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn search_空查询返回全部() {
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");

        let results = model.search("");
        assert_eq!(results.len(), 2);
    }

    // --- records ---

    #[test]
    fn records_初始为空() {
        let model = test_model();
        assert!(model.records().is_empty());
    }
}
