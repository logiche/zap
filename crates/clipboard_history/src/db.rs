//! 剪贴板历史的 SQLite 持久化层
//!
//! 使用 Diesel 管理 schema 与 CRUD 操作。
//!
//! author logic
//! date 2026-05-31

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::sql_query;

use crate::record::{ClipboardRecord, make_preview};

diesel::table! {
    clipboard_history (id) {
        id -> BigInt,
        content -> Text,
        preview -> Text,
        created_at -> Text,
    }
}

use self::clipboard_history::dsl;

/// 数据库连接管理
pub struct ClipboardDb {
    conn: SqliteConnection,
}

/// 查询结果行
#[derive(Queryable, Selectable)]
#[diesel(table_name = clipboard_history)]
struct ClipboardRow {
    id: i64,
    content: String,
    preview: String,
    created_at: String,
}

/// 插入新行
#[derive(Insertable)]
#[diesel(table_name = clipboard_history)]
struct NewClipboardRow<'a> {
    content: &'a str,
    preview: &'a str,
    created_at: &'a str,
}

impl ClipboardDb {
    /// 打开（或创建）数据库
    pub fn open(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = SqliteConnection::establish(&db_path.to_string_lossy())?;
        let mut db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    /// 创建内存数据库（用于测试）
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = SqliteConnection::establish(":memory:")?;
        let mut db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&mut self) -> anyhow::Result<()> {
        sql_query(
            "CREATE TABLE IF NOT EXISTS clipboard_history (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             content TEXT NOT NULL,\
             preview TEXT NOT NULL,\
             created_at TEXT NOT NULL\
             )",
        )
        .execute(&mut self.conn)?;
        Ok(())
    }

    /// 插入一条记录
    pub fn insert(&mut self, content: &str) -> anyhow::Result<ClipboardRecord> {
        let preview = make_preview(content);
        let created_at = Utc::now().to_rfc3339();

        let new_row = NewClipboardRow {
            content,
            preview: &preview,
            created_at: &created_at,
        };

        diesel::insert_into(dsl::clipboard_history)
            .values(&new_row)
            .execute(&mut self.conn)?;

        let row: ClipboardRow = dsl::clipboard_history
            .order(dsl::id.desc())
            .first(&mut self.conn)?;

        Ok(row.to_record())
    }

    /// 查询全部记录（按时间倒序）
    pub fn load_all(&mut self) -> anyhow::Result<Vec<ClipboardRecord>> {
        let rows: Vec<ClipboardRow> = dsl::clipboard_history
            .order(dsl::id.desc())
            .load(&mut self.conn)?;
        Ok(rows.into_iter().map(|r| r.to_record()).collect())
    }

    /// 删除单条记录
    pub fn delete(&mut self, id: i64) -> anyhow::Result<bool> {
        let affected = diesel::delete(dsl::clipboard_history.filter(dsl::id.eq(id)))
            .execute(&mut self.conn)?;
        Ok(affected > 0)
    }

    /// 清空全部记录
    pub fn clear_all(&mut self) -> anyhow::Result<()> {
        diesel::delete(dsl::clipboard_history).execute(&mut self.conn)?;
        Ok(())
    }

    /// 删除超出上限的最旧记录，返回被删除的 ID 列表
    pub fn evict_oldest(&mut self, keep_count: usize) -> anyhow::Result<Vec<i64>> {
        let total: i64 = dsl::clipboard_history.count().get_result(&mut self.conn)?;
        if total as usize <= keep_count {
            return Ok(vec![]);
        }
        let ids_to_delete: Vec<i64> = dsl::clipboard_history
            .order(dsl::id.asc())
            .limit(total as i64 - keep_count as i64)
            .select(dsl::id)
            .load(&mut self.conn)?;
        diesel::delete(dsl::clipboard_history.filter(dsl::id.eq_any(&ids_to_delete)))
            .execute(&mut self.conn)?;
        Ok(ids_to_delete)
    }
}

impl ClipboardRow {
    fn to_record(self) -> ClipboardRecord {
        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        ClipboardRecord {
            id: self.id,
            content: self.content,
            preview: self.preview,
            created_at,
        }
    }
}
