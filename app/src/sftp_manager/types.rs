//! SFTP 管理器 UI 层类型定义
//!
//! author: logic
//! date: 2026-05-26

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 文件条目类型（UI 层）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEntryType {
    File,
    Directory,
    Symlink,
    Other,
}

/// 文件条目（UI 展示用）
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// 文件名
    pub name: String,
    /// 完整路径
    pub path: PathBuf,
    /// 文件类型
    pub file_type: FileEntryType,
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间
    pub modified: Option<String>,
    /// 权限字符串
    pub permissions: Option<String>,
}

/// 传输方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,
    Download,
}

/// 传输状态
#[derive(Debug, Clone)]
pub enum TransferState {
    Pending,
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

/// 传输任务
#[derive(Debug, Clone)]
pub struct TransferTask {
    /// 任务 ID
    pub id: usize,
    /// 源路径
    pub source_path: PathBuf,
    /// 目标路径
    pub target_path: PathBuf,
    /// 传输方向
    pub direction: TransferDirection,
    /// 总大小（字节）
    pub total_size: u64,
    /// 已传输大小（字节）
    pub transferred: u64,
    /// 传输状态
    pub state: TransferState,
    /// 取消标志
    pub cancel_flag: Arc<AtomicBool>,
}

impl TransferTask {
    /// 创建新的传输任务
    pub fn new(
        id: usize,
        source_path: PathBuf,
        target_path: PathBuf,
        direction: TransferDirection,
        total_size: u64,
    ) -> Self {
        Self {
            id,
            source_path,
            target_path,
            direction,
            total_size,
            transferred: 0,
            state: TransferState::Pending,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 计算进度百分比 (0-100)
    pub fn progress_percent(&self) -> u8 {
        if self.total_size == 0 {
            return 0;
        }
        ((self.transferred as f64 / self.total_size as f64) * 100.0) as u8
    }

    /// 取消传输
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// 检查是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}

/// 对话框类型
#[derive(Debug, Clone)]
pub enum Dialog {
    DeleteConfirm { paths: Vec<PathBuf> },
    Rename {
        path: PathBuf,
        original_name: String,
    },
    CreateFolder {
        parent_path: PathBuf,
    },
    Move {
        source: PathBuf,
        target_dir: PathBuf,
    },
    OverwriteConfirm {
        source: PathBuf,
        target: PathBuf,
    },
    FileDetails { entry: FileEntry },
}

/// 连接状态
#[derive(Debug)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Failed(String),
}

/// 格式化文件大小为人类可读字符串
pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}
