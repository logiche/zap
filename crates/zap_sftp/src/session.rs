use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::SftpError;
use crate::sftp::Sftp;

/// 认证方式
#[derive(Debug, Clone)]
pub enum AuthMethod {
    Password { password: String },
    PublicKey { key_path: PathBuf, passphrase: Option<String> },
}

/// SFTP 会话，封装 ssh2 连接
pub struct SftpSession {
    session: Arc<ssh2::Session>,
    _tcp: TcpStream,
}

impl SftpSession {
    /// 通过指定参数建立 SSH 连接
    pub fn connect(
        host: &str,
        port: u16,
        username: &str,
        auth: AuthMethod,
    ) -> Result<Self, SftpError> {
        let addr = format!("{host}:{port}");
        let tcp = TcpStream::connect(&addr)
            .map_err(|e| SftpError::ConnectionFailed(format!("连接 {addr} 失败: {e}")))?;

        let mut session = ssh2::Session::new()
            .map_err(|e| SftpError::ConnectionFailed(format!("创建 SSH 会话失败: {e}")))?;

        let tcp_for_session = tcp.try_clone()
            .map_err(|e| SftpError::ConnectionFailed(format!("克隆 TCP 流失败: {e}")))?;
        session.set_tcp_stream(tcp_for_session);
        session.handshake()
            .map_err(|e| SftpError::ConnectionFailed(format!("SSH 握手失败: {e}")))?;

        match &auth {
            AuthMethod::Password { password } => {
                session.userauth_password(username, password)
                    .map_err(|e| SftpError::AuthFailed(format!("密码认证失败: {e}")))?;
            }
            AuthMethod::PublicKey { key_path, passphrase } => {
                let pass = passphrase.as_deref();
                session.userauth_pubkey_file(username, None, key_path, pass)
                    .map_err(|e| SftpError::AuthFailed(format!("密钥认证失败: {e}")))?;
            }
        }

        if !session.authenticated() {
            return Err(SftpError::AuthFailed("认证未通过".into()));
        }

        Ok(Self {
            session: Arc::new(session),
            _tcp: tcp,
        })
    }

    /// 获取 SFTP 通道
    pub fn sftp(&self) -> Result<Sftp, SftpError> {
        let sftp = self.session.sftp()?;
        Ok(Sftp::new(sftp))
    }

    /// 断开连接
    pub fn disconnect(&self) -> Result<(), SftpError> {
        self.session.disconnect(None, "bye", None)?;
        Ok(())
    }

    /// 检查连接是否存活
    pub fn is_authenticated(&self) -> bool {
        self.session.authenticated()
    }
}

impl Drop for SftpSession {
    fn drop(&mut self) {
        let _ = self.session.disconnect(None, "bye", None);
    }
}
