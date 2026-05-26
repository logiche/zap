//! SFTP 文件浏览器模块
//!
//! 提供 SFTP 连接管理、远程文件浏览、上传下载等功能。
//! author: logic
//! date: 2026-05-26

pub mod breadcrumb;
pub mod browser;
pub mod context_menu;
pub mod dialogs;
pub mod file_list;
pub mod sftp_ops;
pub mod transfer_panel;
pub mod types;

#[allow(unused_imports)]
pub use browser::{SftpBrowserAction, SftpBrowserView};
#[allow(unused_imports)]
pub use types::*;
