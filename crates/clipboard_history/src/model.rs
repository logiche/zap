//! 剪贴板历史 Model（单例）
//!
//! 提供内存缓存 + DB 持久化的统一接口。
//!
//! author logic
//! date 2026-05-31

use std::path::Path;

use warpui::{Entity, SingletonEntity};

use crate::db::ClipboardDb;
use crate::record::ClipboardRecord;

/// 默认最大保留条数
const DEFAULT_MAX_RECORDS: usize = 500;

/// 剪贴板历史单例 Model
pub struct ClipboardHistoryModel {
    db: ClipboardDb,
    records: Vec<ClipboardRecord>,
    max_records: usize,
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
}

impl SingletonEntity for ClipboardHistoryModel {}
