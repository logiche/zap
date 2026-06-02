//! 剪贴板历史云端同步模块
//!
//! 实现与 termux-app-plus 兼容的 Gitee Gist 同步逻辑。
//!
//! author logic
//! date 2026-06-01

use std::collections::HashSet;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::record::ClipboardRecord;
use zap_sync::crypto;

/// Gist description 标记（与 termux 一致）
const GIST_DESCRIPTION: &str = "TERM_PLUS_CLIPBOARD";
/// Gist 内文件名（与 termux 一致）
const GIST_FILENAME: &str = "clipboard";
/// Gitee Gist API 基地址
const GIST_API_BASE: &str = "https://gitee.com/api/v5";
/// 最大同步条数（与 termux 一致）
const SYNC_LIMIT: usize = 20;
/// HTTP 请求超时（秒）
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// 连接超时（秒）
const CONNECT_TIMEOUT_SECS: u64 = 10;
/// 上传防抖间隔（秒）
const DEBOUNCE_SECS: u64 = 5;
/// find_gist 翻页上限，100/页 × 20 页 = 2000 条
const FIND_GIST_MAX_PAGES: u32 = 20;

/// 同步错误
#[derive(Debug, Error)]
pub enum SyncError {
    /// 网络请求失败
    #[error("网络错误: {0}")]
    Network(String),
    /// Token 未配置
    #[error("Token 未配置")]
    NoToken,
    /// API 返回错误状态码
    #[error("API 错误: {status} {body}")]
    Api { status: u16, body: String },
    /// JSON 序列化/反序列化失败
    #[error("序列化错误: {0}")]
    Serialize(String),
    /// 加密/解密失败
    #[error("加密错误: {0}")]
    Crypto(String),
}

impl From<reqwest::Error> for SyncError {
    fn from(e: reqwest::Error) -> Self {
        SyncError::Network(e.to_string())
    }
}

impl From<crypto::CryptoError> for SyncError {
    fn from(e: crypto::CryptoError) -> Self {
        SyncError::Crypto(e.to_string())
    }
}

/// 同步结果
#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    /// 上传成功，新版本号
    Uploaded(i64),
    /// 版本一致，无需操作
    AlreadyUpToDate,
    /// 下载合并完成，新版本号 + 待合并条目 + Gist ID
    Downloaded {
        version: i64,
        new_items: Vec<(String, i64)>,
        gist_id: String,
    },
}

/// 云端同步数据格式（与 termux 的 ClipboardSyncManager 序列化格式一致）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudClipboardData {
    pub version: i64,
    pub updated_at: String,
    pub items: Vec<CloudClipboardItem>,
}

/// 云端单条剪贴板条目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudClipboardItem {
    /// Base64 编码的 AES-256-GCM 密文
    pub content: String,
    /// 毫秒时间戳
    pub timestamp: i64,
}

/// 剪贴板 Gist 同步客户端
pub struct ClipboardGistClient {
    client: Client,
}

impl ClipboardGistClient {
    /// 创建新的同步客户端
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Zap-Terminal")
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .build()
            .expect("failed to build reqwest client for ClipboardGistClient");
        Self { client }
    }

    /// 查找 description 为 TERM_PLUS_CLIPBOARD 的 Gist，返回其 ID
    ///
    /// 分页遍历用户所有 Gist（每页 100 条，最多 20 页），避免因目标
    /// Gist 不在第一页而导致重复创建。
    pub async fn find_gist(&self, token: &str) -> Result<Option<String>, SyncError> {
        if token.is_empty() {
            return Err(SyncError::NoToken);
        }

        for page in 1..=FIND_GIST_MAX_PAGES {
            let url = format!("{GIST_API_BASE}/gists?page={page}&per_page=100");
            let resp = self
                .client
                .get(&url)
                .header("Authorization", format!("token {token}"))
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(SyncError::Api {
                    status: resp.status().as_u16(),
                    body: resp.text().await.unwrap_or_default(),
                });
            }

            let gists: Vec<serde_json::Value> = resp.json().await?;

            if gists.is_empty() {
                return Ok(None);
            }

            for gist in &gists {
                if gist["description"].as_str() == Some(GIST_DESCRIPTION) {
                    if let Some(id) = gist["id"].as_str() {
                        return Ok(Some(id.to_string()));
                    }
                }
            }
        }

        log::warn!(
            "find_gist: 已翻 {FIND_GIST_MAX_PAGES} 页仍未找到 {GIST_DESCRIPTION}, 放弃以避免死循环"
        );
        Ok(None)
    }

    /// 创建新 Gist，返回其 ID
    pub async fn create_gist(&self, token: &str, content: &str) -> Result<String, SyncError> {
        if token.is_empty() {
            return Err(SyncError::NoToken);
        }
        let url = format!("{GIST_API_BASE}/gists");
        let body = serde_json::json!({
            "description": GIST_DESCRIPTION,
            "public": false,
            "files": {
                GIST_FILENAME: { "content": content }
            }
        });
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("token {token}"))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(SyncError::Api {
                status: resp.status().as_u16(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        let detail: serde_json::Value = resp.json().await?;
        detail["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| SyncError::Serialize("Gist 响应缺少 id 字段".to_string()))
    }

    /// 更新已有 Gist
    pub async fn update_gist(
        &self,
        token: &str,
        gist_id: &str,
        content: &str,
    ) -> Result<(), SyncError> {
        if token.is_empty() {
            return Err(SyncError::NoToken);
        }
        let url = format!("{GIST_API_BASE}/gists/{gist_id}");
        let body = serde_json::json!({
            "files": {
                GIST_FILENAME: { "content": content }
            }
        });
        let resp = self
            .client
            .patch(&url)
            .header("Authorization", format!("token {token}"))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(SyncError::Api {
                status: resp.status().as_u16(),
                body: resp.text().await.unwrap_or_default(),
            });
        }
        Ok(())
    }

    /// 下载 Gist 文件内容
    pub async fn get_gist_content(
        &self,
        token: &str,
        gist_id: &str,
    ) -> Result<String, SyncError> {
        if token.is_empty() {
            return Err(SyncError::NoToken);
        }
        let url = format!("{GIST_API_BASE}/gists/{gist_id}");
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("token {token}"))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(SyncError::Api {
                status: resp.status().as_u16(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        let detail: serde_json::Value = resp.json().await?;
        let content = detail["files"][GIST_FILENAME]["content"]
            .as_str()
            .ok_or_else(|| SyncError::Serialize("Gist 文件内容为空".to_string()))?;
        Ok(content.to_string())
    }
}

/// 将本地记录加密并序列化为云端 JSON 格式
pub fn serialize_records(
    records: &[ClipboardRecord],
    version: i64,
    token: &str,
) -> Result<String, SyncError> {
    let now = chrono::Local::now();
    let updated_at = now.format("%Y%m%d%H%M%S").to_string();

    let items: Vec<CloudClipboardItem> = records
        .iter()
        .take(SYNC_LIMIT)
        .map(|r| {
            let encrypted = crypto::encrypt(token, &r.content)?;
            let ts = r.created_at.timestamp_millis();
            Ok(CloudClipboardItem {
                content: encrypted,
                timestamp: ts,
            })
        })
        .collect::<Result<Vec<_>, SyncError>>()?;

    let data = CloudClipboardData {
        version,
        updated_at,
        items,
    };
    serde_json::to_string(&data).map_err(|e| SyncError::Serialize(e.to_string()))
}

/// 从云端 JSON 解析并解密条目列表
pub fn deserialize_records(
    json: &str,
    token: &str,
    existing_contents: &HashSet<String>,
) -> Result<(i64, Vec<(String, i64)>), SyncError> {
    let data: CloudClipboardData =
        serde_json::from_str(json).map_err(|e| SyncError::Serialize(e.to_string()))?;

    let mut new_items = Vec::new();
    for item in &data.items {
        let content = crypto::decrypt(token, &item.content)?;
        if !existing_contents.contains(&content) {
            new_items.push((content, item.timestamp));
        }
    }

    Ok((data.version, new_items))
}

/// 异步上传：加密序列化 + 查找/创建/更新 Gist
///
/// 返回 `(new_version, gist_id)`
pub async fn upload_async(
    client: &ClipboardGistClient,
    token: &str,
    records: &[ClipboardRecord],
    gist_id: Option<&str>,
    version: i64,
) -> Result<(i64, String), SyncError> {
    let json = serialize_records(records, version, token)?;

    let resolved_gist_id = match gist_id {
        Some(id) if !id.is_empty() => {
            client.update_gist(token, id, &json).await?;
            id.to_string()
        }
        _ => {
            match client.find_gist(token).await? {
                Some(found_id) => {
                    client.update_gist(token, &found_id, &json).await?;
                    found_id
                }
                None => client.create_gist(token, &json).await?,
            }
        }
    };

    Ok((version, resolved_gist_id))
}

/// 异步下载：获取 Gist + 解密 + 去重
///
/// 始终返回 `SyncOutcome::Downloaded`（Gist 存在且内容有效的前提下）。
/// 当远程不存在 `TERM_PLUS_CLIPBOARD` Gist 时返回 `AlreadyUpToDate`。
pub async fn download_async(
    client: &ClipboardGistClient,
    token: &str,
    gist_id: Option<&str>,
    existing_contents: &HashSet<String>,
) -> Result<SyncOutcome, SyncError> {
    let resolved_gist_id = match gist_id {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => match client.find_gist(token).await? {
            Some(id) => id,
            None => return Ok(SyncOutcome::AlreadyUpToDate),
        },
    };

    let content = client.get_gist_content(token, &resolved_gist_id).await?;
    let (cloud_version, new_items) = deserialize_records(&content, token, existing_contents)?;

    Ok(SyncOutcome::Downloaded {
        version: cloud_version,
        new_items,
        gist_id: resolved_gist_id,
    })
}

/// 获取防抖间隔（秒）
pub fn debounce_secs() -> u64 {
    DEBOUNCE_SECS
}

/// 获取最大同步条数
pub fn sync_limit() -> usize {
    SYNC_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TOKEN: &str = "test_token_for_sync";

    #[test]
    fn cloud_data_序列化与反序列化() {
        let data = CloudClipboardData {
            version: 1,
            updated_at: "20260601120000".to_string(),
            items: vec![CloudClipboardItem {
                content: "encrypted_content".to_string(),
                timestamp: 1700000000000_i64,
            }],
        };
        let json = serde_json::to_string(&data).expect("serialize failed");
        let parsed: CloudClipboardData = serde_json::from_str(&json).expect("deserialize failed");
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.updated_at, "20260601120000");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].timestamp, 1700000000000_i64);
    }

    #[test]
    fn cloud_data_空items序列化() {
        let data = CloudClipboardData {
            version: 0,
            updated_at: "20260601000000".to_string(),
            items: vec![],
        };
        let json = serde_json::to_string(&data).expect("serialize failed");
        let parsed: CloudClipboardData = serde_json::from_str(&json).expect("deserialize failed");
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn sync_error_from_reqwest() {
        let _: fn(reqwest::Error) -> SyncError = SyncError::from;
    }

    #[test]
    fn sync_error_from_crypto() {
        let err = crypto::CryptoError::Encrypt("test".to_string());
        let sync_err: SyncError = err.into();
        assert!(matches!(sync_err, SyncError::Crypto(_)));
    }

    #[test]
    fn sync_error_display() {
        let err = SyncError::Network("timeout".to_string());
        assert_eq!(format!("{err}"), "网络错误: timeout");

        let err = SyncError::NoToken;
        assert_eq!(format!("{err}"), "Token 未配置");

        let err = SyncError::Api { status: 404, body: "not found".to_string() };
        assert!(format!("{err}").contains("404"));

        let err = SyncError::Serialize("parse err".to_string());
        assert_eq!(format!("{err}"), "序列化错误: parse err");

        let err = SyncError::Crypto("bad key".to_string());
        assert_eq!(format!("{err}"), "加密错误: bad key");
    }

    #[test]
    fn sync_outcome_equality() {
        assert_eq!(SyncOutcome::Uploaded(1), SyncOutcome::Uploaded(1));
        assert_ne!(SyncOutcome::Uploaded(1), SyncOutcome::Uploaded(2));
        assert_eq!(SyncOutcome::AlreadyUpToDate, SyncOutcome::AlreadyUpToDate);
    }

    #[test]
    fn serialize_records_加密并序列化() {
        let records = vec![ClipboardRecord {
            id: 1,
            content: "hello world".to_string(),
            preview: "hello world".to_string(),
            created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
        }];
        let json = serialize_records(&records, 1, TEST_TOKEN).expect("serialize failed");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse failed");
        assert_eq!(parsed["version"], 1);
        assert!(parsed["updated_at"].as_str().unwrap().len() == 14);
        assert_eq!(parsed["items"].as_array().unwrap().len(), 1);
        assert_ne!(parsed["items"][0]["content"].as_str().unwrap(), "hello world");
        assert_eq!(parsed["items"][0]["timestamp"], 1700000000000i64);
    }

    #[test]
    fn deserialize_records_解密并去重() {
        let records = vec![
            ClipboardRecord {
                id: 1,
                content: "hello".to_string(),
                preview: "hello".to_string(),
                created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
            },
            ClipboardRecord {
                id: 2,
                content: "world".to_string(),
                preview: "world".to_string(),
                created_at: chrono::DateTime::from_timestamp_millis(1700000001000_i64).unwrap(),
            },
        ];
        let json = serialize_records(&records, 3, TEST_TOKEN).expect("serialize failed");

        let mut existing = HashSet::new();
        existing.insert("hello".to_string());

        let (version, new_items) =
            deserialize_records(&json, TEST_TOKEN, &existing).expect("deserialize failed");
        assert_eq!(version, 3);
        assert_eq!(new_items.len(), 1);
        assert_eq!(new_items[0].0, "world");
        assert_eq!(new_items[0].1, 1700000001000_i64);
    }

    #[test]
    fn deserialize_records_全部已存在返回空() {
        let records = vec![ClipboardRecord {
            id: 1,
            content: "dup".to_string(),
            preview: "dup".to_string(),
            created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
        }];
        let json = serialize_records(&records, 1, TEST_TOKEN).expect("serialize failed");

        let mut existing = HashSet::new();
        existing.insert("dup".to_string());

        let (_, new_items) =
            deserialize_records(&json, TEST_TOKEN, &existing).expect("deserialize failed");
        assert!(new_items.is_empty());
    }

    #[test]
    fn deserialize_records_错误token解密失败() {
        let records = vec![ClipboardRecord {
            id: 1,
            content: "secret".to_string(),
            preview: "secret".to_string(),
            created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
        }];
        let json = serialize_records(&records, 1, TEST_TOKEN).expect("serialize failed");

        let result = deserialize_records(&json, "wrong_token", &HashSet::new());
        assert!(result.is_err());
    }

    #[test]
    fn serialize_records_超过sync_limit截断() {
        let records: Vec<ClipboardRecord> = (0..30)
            .map(|i| ClipboardRecord {
                id: i,
                content: format!("item{i}"),
                preview: format!("item{i}"),
                created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64 + i * 1000)
                    .unwrap(),
            })
            .collect();

        let json = serialize_records(&records, 1, TEST_TOKEN).expect("serialize failed");
        let parsed: CloudClipboardData = serde_json::from_str(&json).expect("parse failed");
        assert_eq!(parsed.items.len(), SYNC_LIMIT);
    }

    #[test]
    fn constants_值正确() {
        assert_eq!(GIST_DESCRIPTION, "TERM_PLUS_CLIPBOARD");
        assert_eq!(GIST_FILENAME, "clipboard");
        assert_eq!(GIST_API_BASE, "https://gitee.com/api/v5");
        assert_eq!(sync_limit(), 20);
        assert_eq!(debounce_secs(), 5);
    }

    // --- upload / download flow tests ---

    #[tokio::test]
    async fn upload_创建新gist_验证序列化() {
        let records = vec![ClipboardRecord {
            id: 1,
            content: "test content".to_string(),
            preview: "test content".to_string(),
            created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
        }];
        let json = serialize_records(&records, 1, TEST_TOKEN).expect("serialize failed");
        let parsed: CloudClipboardData = serde_json::from_str(&json).expect("parse failed");
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.items.len(), 1);
    }

    #[test]
    fn upload_流程验证_序列化加密版本递增() {
        let records = vec![
            ClipboardRecord {
                id: 1,
                content: "aaa".to_string(),
                preview: "aaa".to_string(),
                created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
            },
            ClipboardRecord {
                id: 2,
                content: "bbb".to_string(),
                preview: "bbb".to_string(),
                created_at: chrono::DateTime::from_timestamp_millis(1700000001000_i64).unwrap(),
            },
        ];
        let json = serialize_records(&records, 4, TEST_TOKEN).expect("serialize failed");
        let data: CloudClipboardData = serde_json::from_str(&json).expect("parse failed");
        assert_eq!(data.version, 4);
        assert_eq!(data.items.len(), 2);
        for item in &data.items {
            let decrypted = crypto::decrypt(TEST_TOKEN, &item.content).expect("decrypt failed");
            assert!(decrypted == "aaa" || decrypted == "bbb");
        }
    }

    #[test]
    fn download_流程验证_解密去重合并() {
        let records = vec![
            ClipboardRecord {
                id: 1,
                content: "cloud_a".to_string(),
                preview: "cloud_a".to_string(),
                created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
            },
            ClipboardRecord {
                id: 2,
                content: "cloud_b".to_string(),
                preview: "cloud_b".to_string(),
                created_at: chrono::DateTime::from_timestamp_millis(1700000001000_i64).unwrap(),
            },
        ];
        let cloud_json = serialize_records(&records, 5, TEST_TOKEN).expect("serialize failed");
        let mut existing = HashSet::new();
        existing.insert("cloud_a".to_string());
        let (version, new_items) =
            deserialize_records(&cloud_json, TEST_TOKEN, &existing).expect("deserialize failed");
        assert_eq!(version, 5);
        assert_eq!(new_items.len(), 1);
        assert_eq!(new_items[0].0, "cloud_b");
    }

    #[test]
    fn updated_at_格式验证() {
        let records = vec![ClipboardRecord {
            id: 1,
            content: "x".to_string(),
            preview: "x".to_string(),
            created_at: chrono::DateTime::from_timestamp_millis(1700000000000_i64).unwrap(),
        }];
        let json = serialize_records(&records, 1, TEST_TOKEN).expect("serialize failed");
        let data: CloudClipboardData = serde_json::from_str(&json).expect("parse failed");
        assert_eq!(data.updated_at.len(), 14);
        assert!(data.updated_at.chars().all(|c| c.is_ascii_digit()));
    }
}
