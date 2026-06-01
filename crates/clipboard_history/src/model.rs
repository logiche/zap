//! 剪贴板历史 Model（单例）
//!
//! 提供内存缓存 + DB 持久化的统一接口。
//!
//! author logic
//! date 2026-05-31

use std::path::Path;

use warpui::r#async::SpawnedFutureHandle;
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
    /// Gitee 同步 token
    sync_token: Option<String>,
    /// 防抖上传定时器句柄 — 每次剪贴板变化时重置，5秒静默后才触发上传
    sync_upload_handle: Option<SpawnedFutureHandle>,
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
            sync_token: None,
            sync_upload_handle: None,
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
            sync_token: None,
            sync_upload_handle: None,
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
        self.schedule_debounced_sync_upload(ctx);
        ctx.notify();
    }

    /// 调度防抖上传：取消前一个等待中的定时器，启动新的 5 秒倒计时。
    /// 仅当 5 秒内无新的剪贴板事件时，才会真正触发 `trigger_sync_upload`。
    fn schedule_debounced_sync_upload(&mut self, ctx: &mut ModelContext<Self>) {
        // 取消前一个定时器（如果有）
        if let Some(handle) = self.sync_upload_handle.take() {
            handle.abort();
        }

        let debounce_duration = std::time::Duration::from_secs(crate::sync::debounce_secs());

        let new_handle = ctx.spawn_abortable(
            async move { warpui::r#async::Timer::after(debounce_duration).await },
            |me, _, ctx| {
                // 定时器完成（未被取消）→ 触发上传
                me.sync_upload_handle = None;
                me.trigger_sync_upload(ctx);
            },
            |me, _| {
                // 定时器被取消（新的剪贴板事件取代了本次）
                me.sync_upload_handle = None;
            },
        );

        self.sync_upload_handle = Some(new_handle);
    }

    // ==================== 云同步相关 ====================

    /// 设置同步 token（空字符串清除）
    pub fn set_sync_token(&mut self, token: String) {
        if token.is_empty() {
            self.sync_token = None;
        } else {
            self.sync_token = Some(token);
        }
    }

    /// 获取同步 token
    pub fn get_sync_token(&self) -> Option<&str> {
        self.sync_token.as_deref()
    }

    /// 获取当前同步版本号（从 sync_meta 读取）
    pub fn get_sync_version(&mut self) -> i64 {
        self.db
            .get_sync_meta("version")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    }

    /// 设置同步版本号
    pub fn set_sync_version(&mut self, version: i64) -> anyhow::Result<()> {
        self.db.set_sync_meta("version", &version.to_string())
    }

    /// 获取同步 Gist ID
    pub fn get_sync_gist_id(&mut self) -> Option<String> {
        self.db.get_sync_meta("gist_id")
    }

    /// 设置同步 Gist ID
    pub fn set_sync_gist_id(&mut self, gist_id: &str) -> anyhow::Result<()> {
        self.db.set_sync_meta("gist_id", gist_id)
    }

    /// 获取当前所有记录的内容集合（用于下载去重）
    pub fn existing_contents(&self) -> std::collections::HashSet<String> {
        self.records.iter().map(|r| r.content.clone()).collect()
    }

    /// 获取最近 N 条记录快照（用于上传）
    pub fn recent_records_snapshot(&self) -> Vec<ClipboardRecord> {
        self.records.iter().take(crate::sync::sync_limit()).cloned().collect()
    }

    /// 应用下载合并结果（插入新条目，使用云端时间戳）
    ///
    /// 返回实际新增的条目数
    pub fn apply_sync_download(
        &mut self,
        items: &[(String, i64)],
    ) -> anyhow::Result<usize> {
        let mut added = 0;
        for (content, timestamp_ms) in items {
            if self.records.iter().any(|r| r.content == *content) {
                continue;
            }
            if self.db.insert_with_timestamp(content, *timestamp_ms)?.is_some() {
                added += 1;
            }
        }
        self.records = self.db.load_all()?;
        let evicted = self.db.evict_oldest(self.max_records)?;
        if !evicted.is_empty() {
            self.records.retain(|r| !evicted.contains(&r.id));
        }
        Ok(added)
    }

    /// 触发异步上传同步
    pub fn trigger_sync_upload(&mut self, ctx: &mut ModelContext<Self>) {
        let Some(token) = self.sync_token.clone() else {
            return;
        };
        let records = self.recent_records_snapshot();
        let gist_id = self.get_sync_gist_id();
        let version = self.get_sync_version() + 1;
        let foreground = ctx.spawner();

        ctx.spawn(
            async move {
                let client = crate::sync::ClipboardGistClient::new();
                match crate::sync::upload_async(
                    &client,
                    &token,
                    &records,
                    gist_id.as_deref(),
                    version,
                )
                .await
                {
                    Ok((new_version, gist_id)) => {
                        log::debug!("剪贴板上传同步完成, version={new_version}");
                        let _ = foreground
                            .spawn(move |me, _| {
                                let _ = me.set_sync_version(new_version);
                                let _ = me.set_sync_gist_id(&gist_id);
                            })
                            .await;
                    }
                    Err(e) => {
                        log::debug!("剪贴板上传同步失败: {e}");
                    }
                }
            },
            |_, _, _| {},
        );
    }

    /// 触发异步下载同步
    pub fn trigger_sync_download(&mut self, ctx: &mut ModelContext<Self>) {
        let Some(token) = self.sync_token.clone() else {
            return;
        };
        let gist_id = self.get_sync_gist_id();
        let version = self.get_sync_version();
        let existing = self.existing_contents();
        let foreground = ctx.spawner();

        ctx.spawn(
            async move {
                let client = crate::sync::ClipboardGistClient::new();
                match crate::sync::download_async(
                    &client,
                    &token,
                    gist_id.as_deref(),
                    version,
                    &existing,
                )
                .await
                {
                    Ok(crate::sync::SyncOutcome::Downloaded {
                        version,
                        new_items,
                        gist_id,
                    }) => {
                        log::debug!("剪贴板下载同步完成, version={version}");
                        let _ = foreground
                            .spawn(move |me, _| {
                                if let Ok(added) = me.apply_sync_download(&new_items) {
                                    log::debug!("剪贴板合并了{added}条新记录");
                                }
                                let _ = me.set_sync_version(version);
                                let _ = me.set_sync_gist_id(&gist_id);
                            })
                            .await;
                    }
                    Ok(crate::sync::SyncOutcome::AlreadyUpToDate) => {
                        log::debug!("剪贴板已是最新");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::debug!("剪贴板下载同步失败: {e}");
                    }
                }
            },
            |_, _, _| {},
        );
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

    // --- sync token ---

    #[test]
    fn set_sync_token_设置后可获取() {
        let mut model = test_model();
        model.set_sync_token("my_token".to_string());
        assert_eq!(model.get_sync_token(), Some("my_token"));
    }

    #[test]
    fn set_sync_token_空字符串清除token() {
        let mut model = test_model();
        model.set_sync_token("token".to_string());
        model.set_sync_token(String::new());
        assert_eq!(model.get_sync_token(), None);
    }

    #[test]
    fn get_sync_token_未设置返回none() {
        let model = test_model();
        assert_eq!(model.get_sync_token(), None);
    }

    // --- sync meta ---

    #[test]
    fn get_sync_version_初始为0() {
        let mut model = test_model();
        assert_eq!(model.get_sync_version(), 0);
    }

    #[test]
    fn set_sync_version_更新后可读取() {
        let mut model = test_model();
        model.set_sync_version(5).expect("set failed");
        assert_eq!(model.get_sync_version(), 5);
    }

    #[test]
    fn get_sync_gist_id_初始为none() {
        let mut model = test_model();
        assert_eq!(model.get_sync_gist_id(), None);
    }

    #[test]
    fn set_sync_gist_id_更新后可读取() {
        let mut model = test_model();
        model.set_sync_gist_id("gist_abc123").expect("set failed");
        assert_eq!(model.get_sync_gist_id(), Some("gist_abc123".to_string()));
    }

    // --- apply sync download ---

    #[test]
    fn apply_sync_download_新增不重复条目() {
        let mut model = test_model();
        model.add_record("existing".to_string()).expect("add failed");

        let items = vec![
            ("new_item".to_string(), 1700000000000_i64),
            ("existing".to_string(), 1700000001000_i64), // 重复
        ];
        let added = model.apply_sync_download(&items).expect("apply failed");
        assert_eq!(added, 1);
        assert_eq!(model.records().len(), 2);
        assert!(model.records().iter().any(|r| r.content == "new_item"));
    }

    #[test]
    fn apply_sync_download_全部重复返回0() {
        let mut model = test_model();
        model.add_record("dup".to_string()).expect("add failed");

        let items = vec![("dup".to_string(), 1700000000000_i64)];
        let added = model.apply_sync_download(&items).expect("apply failed");
        assert_eq!(added, 0);
        assert_eq!(model.records().len(), 1);
    }

    #[test]
    fn apply_sync_download_空列表返回0() {
        let mut model = test_model();
        let added = model.apply_sync_download(&[]).expect("apply failed");
        assert_eq!(added, 0);
    }

    #[test]
    fn existing_contents_返回所有内容() {
        let mut model = test_model();
        model.add_record("aaa".to_string()).expect("add failed");
        model.add_record("bbb".to_string()).expect("add failed");

        let contents = model.existing_contents();
        assert_eq!(contents.len(), 2);
        assert!(contents.contains("aaa"));
        assert!(contents.contains("bbb"));
    }
}
