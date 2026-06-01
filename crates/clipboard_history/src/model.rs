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
const DEFAULT_MAX_RECORDS: usize = 20;

/// 剪贴板历史单例 Model
pub struct ClipboardHistoryModel {
    db: ClipboardDb,
    records: Vec<ClipboardRecord>,
    max_records: usize,
    watcher: Option<ClipboardWatcher>,
    /// 引用计数：跟踪有多少个 Pane 在使用 watcher
    watcher_ref_count: usize,
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
            watcher_ref_count: 0,
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
            watcher_ref_count: 0,
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
        // 如果内容已存在，移到顶部而非重复插入
        if let Some(pos) = self.records.iter().position(|r| r.content == content) {
            if pos == 0 {
                return Ok(None); // 已在顶部，无需操作
            }
            let old_id = self.records[pos].id;
            self.db.delete(old_id)?;
            self.records.remove(pos);
            // 继续执行下方的 insert
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

    /// 启动剪贴板监听（事件驱动，引用计数）
    ///
    /// 后台线程通过 Windows 原生 API 监听剪贴板变化，
    /// 仅在内容实际改变时读取并通知。
    /// 首次调用时真正启动 watcher，后续调用仅递增引用计数。
    pub fn start_watching(&mut self, ctx: &mut ModelContext<Self>) {
        self.watcher_ref_count = self.watcher_ref_count.saturating_add(1);
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

    /// 停止剪贴板监听（引用计数）
    ///
    /// 递减引用计数，仅在归零时真正停止 watcher。
    pub fn stop_watching(&mut self) {
        if self.watcher_ref_count > 0 {
            self.watcher_ref_count -= 1;
        }
        if self.watcher_ref_count == 0 {
            if let Some(mut watcher) = self.watcher.take() {
                watcher.stop();
            }
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
    fn add_record_重复旧记录移到顶部() {
        let mut model = test_model();

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");
        // "a" 不再是最近一条，重新添加应该移到顶部而非新增
        let event = model.add_record("a".to_string()).expect("add failed");
        assert!(event.is_some());
        assert_eq!(model.records().len(), 2); // 仍然是 2 条
        assert_eq!(model.records()[0].content, "a"); // "a" 移到顶部
        assert_eq!(model.records()[1].content, "b");
    }

    #[test]
    fn add_record_已在顶部的记录不产生事件() {
        let mut model = test_model();

        model.add_record("top".to_string()).expect("add failed");
        let event = model.add_record("top".to_string()).expect("add failed");
        assert!(event.is_none());
        assert_eq!(model.records().len(), 1);
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

    // --- 边界测试 ---

    #[test]
    fn add_record_淘汰超出上限的旧记录() {
        let mut model = test_model();
        // 设置较小的上限以触发淘汰
        model.max_records = 3;

        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");
        model.add_record("c".to_string()).expect("add failed");
        assert_eq!(model.records().len(), 3);

        // 添加第 4 条，应淘汰最旧的 "a"
        model.add_record("d".to_string()).expect("add failed");
        assert_eq!(model.records().len(), 3);
        assert!(model.records().iter().all(|r| r.content != "a"));
        assert_eq!(model.records()[0].content, "d");
    }

    #[test]
    fn add_record_多条淘汰仅保留最新() {
        let mut model = test_model();
        model.max_records = 2;

        for i in 0..5 {
            model.add_record(format!("item{i}")).expect("add failed");
        }

        assert_eq!(model.records().len(), 2);
        // 最新的两条保留
        assert_eq!(model.records()[0].content, "item4");
        assert_eq!(model.records()[1].content, "item3");
    }

    #[test]
    fn new_in_memory_创建成功() {
        let model = ClipboardHistoryModel::new_in_memory();
        assert!(model.is_ok());
        assert!(model.unwrap().records().is_empty());
    }

    #[test]
    fn search_空模型搜索返回空() {
        let model = test_model();
        let results = model.search("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn search_空查询返回全部记录() {
        let mut model = test_model();
        model.add_record("a".to_string()).expect("add failed");
        model.add_record("b".to_string()).expect("add failed");
        model.add_record("c".to_string()).expect("add failed");

        let results = model.search("");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn delete_删除后其他记录保留() {
        let mut model = test_model();
        model.add_record("keep1".to_string()).expect("add failed");
        model.add_record("delete_me".to_string()).expect("add failed");
        model.add_record("keep2".to_string()).expect("add failed");

        let target_id = model.records().iter().find(|r| r.content == "delete_me").unwrap().id;
        model.delete(target_id).expect("delete failed");

        assert_eq!(model.records().len(), 2);
        assert!(model.records().iter().any(|r| r.content == "keep1"));
        assert!(model.records().iter().any(|r| r.content == "keep2"));
    }

    #[test]
    fn clear_all_空模型安全处理() {
        let mut model = test_model();
        let event = model.clear_all().expect("clear_all failed");
        assert!(matches!(event, ClipboardHistoryModelEvent::AllCleared));
        assert!(model.records().is_empty());
    }

    #[test]
    fn add_record_纯空白字符串返回none() {
        let mut model = test_model();
        let event = model.add_record("   ".to_string()).expect("add failed");
        // "   " 不是空字符串，is_empty() 为 false，所以会添加
        assert!(event.is_some());
    }

    #[test]
    fn add_record_包含unicode内容正常添加() {
        let mut model = test_model();
        let content = "你好世界 🌍 𝕳𝖊𝖑𝖑𝖔".to_string();
        let event = model.add_record(content.clone()).expect("add failed");
        assert!(event.is_some());
        assert_eq!(model.records()[0].content, content);
    }
}
