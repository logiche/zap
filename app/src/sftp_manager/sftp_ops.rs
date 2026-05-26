//! SFTP 操作封装层
//!
//! 将 zap_sftp 协议层 API 封装为 UI 层可直接使用的高级操作。
//! author: logic
//! date: 2026-05-26

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use zap_sftp::session::{AuthMethod, SftpSession};
use zap_sftp::types::OpenOptions;
use zap_sftp::Sftp;
use warp_ssh_manager::secrets::{SecretKind, SshSecretStore};
use warp_ssh_manager::types::{AuthType, SshServerInfo};

use super::types::{FileEntry, FileEntryType};

/// SFTP 操作错误
#[derive(Debug)]
pub enum SftpOpsError {
    /// 连接错误
    Connection(String),
    /// 操作错误
    Operation(String),
    /// 本地 IO 错误
    LocalIo(String),
    /// 未找到凭据
    NoCredentials(String),
    /// 传输已取消
    Cancelled,
}

impl std::fmt::Display for SftpOpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SftpOpsError::Connection(msg) => write!(f, "连接错误: {msg}"),
            SftpOpsError::Operation(msg) => write!(f, "操作错误: {msg}"),
            SftpOpsError::LocalIo(msg) => write!(f, "本地 IO 错误: {msg}"),
            SftpOpsError::NoCredentials(msg) => write!(f, "未找到凭据: {msg}"),
            SftpOpsError::Cancelled => write!(f, "传输已取消"),
        }
    }
}

impl From<zap_sftp::SftpError> for SftpOpsError {
    fn from(e: zap_sftp::SftpError) -> Self {
        SftpOpsError::Operation(e.to_string())
    }
}

impl From<std::io::Error> for SftpOpsError {
    fn from(e: std::io::Error) -> Self {
        SftpOpsError::LocalIo(e.to_string())
    }
}

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send>;

/// 使用服务器配置建立 SFTP 连接
pub fn connect_from_server(
    server: &SshServerInfo,
    secret_store: &dyn SshSecretStore,
) -> Result<SftpSession, SftpOpsError> {
    let auth = build_auth_method(server, secret_store)?;
    SftpSession::connect(&server.host, server.port, &server.username, auth)
        .map_err(|e| SftpOpsError::Connection(e.to_string()))
}

/// 列出远程目录内容，转换为 UI 层 FileEntry
pub fn list_dir(sftp: &Sftp, path: &Path) -> Result<Vec<FileEntry>, SftpOpsError> {
    let entries = sftp.read_dir(path)?;
    let result = entries
        .into_iter()
        .map(|entry| {
            let file_type = match entry.metadata.file_type {
                zap_sftp::types::FileType::Dir => FileEntryType::Directory,
                zap_sftp::types::FileType::File => FileEntryType::File,
                zap_sftp::types::FileType::Symlink => FileEntryType::Symlink,
                zap_sftp::types::FileType::Other => FileEntryType::Other,
            };
            let modified = entry.metadata.modified.map(|t| {
                let datetime: chrono::DateTime<chrono::Local> = t.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            });
            let perms = &entry.metadata.permissions;
            let permissions = Some(format!(
                "{}{}{}{}{}{}{}{}{}",
                if perms.owner_read { 'r' } else { '-' },
                if perms.owner_write { 'w' } else { '-' },
                if perms.owner_exec { 'x' } else { '-' },
                if perms.group_read { 'r' } else { '-' },
                if perms.group_write { 'w' } else { '-' },
                if perms.group_exec { 'x' } else { '-' },
                if perms.other_read { 'r' } else { '-' },
                if perms.other_write { 'w' } else { '-' },
                if perms.other_exec { 'x' } else { '-' },
            ));
            FileEntry {
                name: entry.name,
                path: entry.path,
                file_type,
                size: entry.metadata.size,
                modified,
                permissions,
            }
        })
        .collect();
    Ok(result)
}

/// 删除远程文件
pub fn delete_file(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    sftp.remove_file(path)?;
    Ok(())
}

/// 递归删除远程目录
pub fn delete_dir_recursive(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    let entries = sftp.read_dir(path)?;
    for entry in entries {
        match entry.metadata.file_type {
            zap_sftp::types::FileType::Dir => {
                delete_dir_recursive(sftp, &entry.path)?;
            }
            _ => {
                sftp.remove_file(&entry.path)?;
            }
        }
    }
    sftp.remove_dir(path)?;
    Ok(())
}

/// 创建远程目录
pub fn create_dir(sftp: &Sftp, path: &Path) -> Result<(), SftpOpsError> {
    sftp.create_dir(path)?;
    Ok(())
}

/// 重命名远程文件或目录
pub fn rename(sftp: &Sftp, old_path: &Path, new_path: &Path) -> Result<(), SftpOpsError> {
    let opts = zap_sftp::types::RenameOptions {
        overwrite: false,
        atomic: false,
        native: false,
    };
    sftp.rename(old_path, new_path, opts)?;
    Ok(())
}

/// 流式上传本地文件到远程
pub fn upload_file_streaming(
    sftp: &Sftp,
    local_path: &Path,
    remote_path: &Path,
    progress_cb: Option<&ProgressCallback>,
) -> Result<(), SftpOpsError> {
    let mut local_file =
        fs::File::open(local_path).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    let total_size = local_file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut remote_file = sftp.open(remote_path, OpenOptions::write())?;

    const CHUNK_SIZE: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    loop {
        let n = std::io::Read::read(&mut local_file, &mut buf)
            .map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
        if n == 0 {
            break;
        }
        remote_file.write_all(&buf[..n])?;
        transferred += n as u64;
        if let Some(cb) = progress_cb {
            cb(transferred, total_size);
        }
    }

    remote_file.flush()?;
    Ok(())
}

/// 流式下载远程文件到本地
pub fn download_file_streaming(
    sftp: &Sftp,
    remote_path: &Path,
    local_path: &Path,
    progress_cb: Option<&ProgressCallback>,
) -> Result<(), SftpOpsError> {
    let mut remote_file = sftp.open(remote_path, OpenOptions::read())?;
    let metadata = remote_file.stat()?;
    let total_size = metadata.size;

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    }

    let mut local_file =
        fs::File::create(local_path).map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;

    const CHUNK_SIZE: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    loop {
        let n = remote_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        local_file
            .write_all(&buf[..n])
            .map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
        transferred += n as u64;
        if let Some(cb) = progress_cb {
            cb(transferred, total_size);
        }
    }

    local_file.flush().map_err(|e| SftpOpsError::LocalIo(e.to_string()))?;
    Ok(())
}

/// 根据服务器配置构建认证方式
fn build_auth_method(
    server: &SshServerInfo,
    secret_store: &dyn SshSecretStore,
) -> Result<AuthMethod, SftpOpsError> {
    match server.auth_type {
        AuthType::Password => {
            let password = secret_store
                .get(&server.node_id, SecretKind::Password)
                .map_err(|e| SftpOpsError::NoCredentials(format!("读取密码失败: {e}")))?
                .ok_or_else(|| {
                    SftpOpsError::NoCredentials(format!(
                        "服务器 {} 未存储密码",
                        server.host
                    ))
                })?;
            Ok(AuthMethod::Password {
                password: password.to_string(),
            })
        }
        AuthType::Key => {
            let key_path = server
                .key_path
                .as_ref()
                .ok_or_else(|| {
                    SftpOpsError::NoCredentials("密钥认证但未指定密钥路径".to_string())
                })?;
            let expanded = shellexpand_path(key_path);
            let passphrase = secret_store
                .get(&server.node_id, SecretKind::Passphrase)
                .ok()
                .flatten()
                .map(|p| p.to_string());
            Ok(AuthMethod::PublicKey {
                key_path: PathBuf::from(expanded),
                passphrase,
            })
        }
    }
}

/// 展开路径中的 ~ 为用户主目录
fn shellexpand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), &path[2..]);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 SftpOpsError::Connection Display 输出
    #[test]
    fn test_sftp_ops_error_display_connection() {
        assert_eq!(
            SftpOpsError::Connection("refused".into()).to_string(),
            "连接错误: refused"
        );
    }

    /// 测试 SftpOpsError::Operation Display 输出
    #[test]
    fn test_sftp_ops_error_display_operation() {
        assert_eq!(
            SftpOpsError::Operation("not found".into()).to_string(),
            "操作错误: not found"
        );
    }

    /// 测试 SftpOpsError::LocalIo Display 输出
    #[test]
    fn test_sftp_ops_error_display_local_io() {
        assert_eq!(
            SftpOpsError::LocalIo("disk full".into()).to_string(),
            "本地 IO 错误: disk full"
        );
    }

    /// 测试 SftpOpsError::NoCredentials Display 输出
    #[test]
    fn test_sftp_ops_error_display_no_credentials() {
        assert_eq!(
            SftpOpsError::NoCredentials("no key".into()).to_string(),
            "未找到凭据: no key"
        );
    }

    /// 测试 SftpOpsError::Cancelled Display 输出
    #[test]
    fn test_sftp_ops_error_display_cancelled() {
        assert_eq!(SftpOpsError::Cancelled.to_string(), "传输已取消");
    }

    /// 测试从 std::io::Error 转换为 SftpOpsError
    #[test]
    fn test_sftp_ops_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ops_err: SftpOpsError = io_err.into();
        assert!(matches!(ops_err, SftpOpsError::LocalIo(_)));
    }

    /// 测试从 zap_sftp::SftpError 转换为 SftpOpsError
    #[test]
    fn test_sftp_ops_error_from_sftp_error() {
        let sftp_err = zap_sftp::SftpError::General("test error".into());
        let ops_err: SftpOpsError = sftp_err.into();
        assert!(matches!(ops_err, SftpOpsError::Operation(_)));
    }

    /// 测试 shellexpand_path 展开 ~/ 路径
    #[test]
    fn test_shellexpand_path_home() {
        let home = dirs::home_dir().unwrap_or_default();
        let result = shellexpand_path("~/test");
        if !home.as_os_str().is_empty() {
            assert!(!result.starts_with('~'));
            assert!(result.contains("test"));
        }
    }

    /// 测试 shellexpand_path 不变绝对路径
    #[test]
    fn test_shellexpand_path_absolute() {
        let result = shellexpand_path("/absolute/path");
        assert_eq!(result, "/absolute/path");
    }

    /// 测试 shellexpand_path 不变相对路径
    #[test]
    fn test_shellexpand_path_relative() {
        let result = shellexpand_path("relative/path");
        assert_eq!(result, "relative/path");
    }

    /// 测试 shellexpand_path 仅 ~ 不展开
    #[test]
    fn test_shellexpand_path_tilde_only() {
        let result = shellexpand_path("~");
        assert_eq!(result, "~");
    }

    /// 测试 shellexpand_path 空路径
    #[test]
    fn test_shellexpand_path_empty() {
        let result = shellexpand_path("");
        assert_eq!(result, "");
    }
}
