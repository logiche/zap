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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    /// 测试 format_size 零字节
    #[test]
    fn test_format_size_zero() {
        assert_eq!(format_size(0), "0 B");
    }

    /// 测试 format_size 字节级别
    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    /// 测试 format_size KB 级别
    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 512), "512.0 KB");
    }

    /// 测试 format_size MB 级别
    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024 + 512 * 1024), "2.5 MB");
    }

    /// 测试 format_size GB 级别
    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    /// 测试 TransferTask 新建
    #[test]
    fn test_transfer_task_new() {
        let task = TransferTask::new(
            1,
            PathBuf::from("/remote/file.txt"),
            PathBuf::from("/local/file.txt"),
            TransferDirection::Download,
            1024,
        );
        assert_eq!(task.id, 1);
        assert_eq!(task.total_size, 1024);
        assert_eq!(task.transferred, 0);
        assert!(matches!(task.state, TransferState::Pending));
        assert!(!task.is_cancelled());
    }

    /// 测试 TransferTask 总大小为零时进度
    #[test]
    fn test_transfer_task_progress_zero() {
        let task = TransferTask::new(
            1,
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            TransferDirection::Upload,
            0,
        );
        assert_eq!(task.progress_percent(), 0);
    }

    /// 测试 TransferTask 50% 进度
    #[test]
    fn test_transfer_task_progress_half() {
        let mut task = TransferTask::new(
            1,
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            TransferDirection::Upload,
            1000,
        );
        task.transferred = 500;
        assert_eq!(task.progress_percent(), 50);
    }

    /// 测试 TransferTask 100% 进度
    #[test]
    fn test_transfer_task_progress_full() {
        let mut task = TransferTask::new(
            1,
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            TransferDirection::Download,
            1000,
        );
        task.transferred = 1000;
        assert_eq!(task.progress_percent(), 100);
    }

    /// 测试 TransferTask 进度百分比取整
    #[test]
    fn test_transfer_task_progress_rounding() {
        let mut task = TransferTask::new(
            1,
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            TransferDirection::Upload,
            3,
        );
        task.transferred = 1;
        assert_eq!(task.progress_percent(), 33);
    }

    /// 测试 TransferTask 取消操作
    #[test]
    fn test_transfer_task_cancel() {
        let task = TransferTask::new(
            1,
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            TransferDirection::Upload,
            100,
        );
        assert!(!task.is_cancelled());
        task.cancel();
        assert!(task.is_cancelled());
    }

    /// 测试 TransferTask 取消标志共享
    #[test]
    fn test_transfer_task_cancel_flag_shared() {
        let task = TransferTask::new(
            1,
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            TransferDirection::Download,
            100,
        );
        let flag = task.cancel_flag.clone();
        flag.store(true, Ordering::SeqCst);
        assert!(task.is_cancelled());
    }

    /// 测试 FileEntryType 相等性
    #[test]
    fn test_file_entry_type_equality() {
        assert_eq!(FileEntryType::File, FileEntryType::File);
        assert_eq!(FileEntryType::Directory, FileEntryType::Directory);
        assert_ne!(FileEntryType::File, FileEntryType::Directory);
    }

    /// 测试 TransferDirection 相等性
    #[test]
    fn test_transfer_direction_equality() {
        assert_eq!(TransferDirection::Upload, TransferDirection::Upload);
        assert_eq!(TransferDirection::Download, TransferDirection::Download);
        assert_ne!(TransferDirection::Upload, TransferDirection::Download);
    }

    /// 测试 ConnectionState Debug 输出
    #[test]
    fn test_connection_state_debug() {
        let states = vec![
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Disconnected,
            ConnectionState::Failed("timeout".into()),
        ];
        for state in &states {
            let debug_str = format!("{state:?}");
            assert!(!debug_str.is_empty());
        }
    }

    /// 测试 Dialog 枚举变体
    #[test]
    fn test_dialog_variants() {
        let delete = Dialog::DeleteConfirm {
            paths: vec![PathBuf::from("/foo")],
        };
        assert!(matches!(delete, Dialog::DeleteConfirm { .. }));

        let rename = Dialog::Rename {
            path: PathBuf::from("/old"),
            original_name: "old".into(),
        };
        assert!(matches!(rename, Dialog::Rename { .. }));

        let folder = Dialog::CreateFolder {
            parent_path: PathBuf::from("/home"),
        };
        assert!(matches!(folder, Dialog::CreateFolder { .. }));

        let details = Dialog::FileDetails {
            entry: FileEntry {
                name: "test.txt".into(),
                path: PathBuf::from("/test.txt"),
                file_type: FileEntryType::File,
                size: 100,
                modified: None,
                permissions: None,
            },
        };
        assert!(matches!(details, Dialog::FileDetails { .. }));
    }
}
