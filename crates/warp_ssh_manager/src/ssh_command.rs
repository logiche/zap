//! 把 `SshServerInfo` 拼成 `ssh ...` 命令,并派生测试连接的子进程。
//!
//! 写入 PTY 时调 `build_ssh_command_line`,会用 shell-escape 引用每个 arg,
//! 防止用户名 / host / key_path 里的空格或单引号破坏命令行。
//!
//! ## 密码认证安全
//!
//! 不依赖外部 `sshpass` 二进制(Windows / macOS 默认不存在),改用直接派生 `ssh`
//! 子进程 + 一次性 stdin 注入。`ssh` 通过 `PreferredAuthentications=password` +
//! `NumberOfPasswordPrompts=1` 强制只走密码且只尝试一次,匹配后立即 drop stdin
//! 让 ssh 端 EOF。这样密码全程只在内存中以 `Zeroizing<String>` 形式持有,**不会**
//! 进入 argv → 不会出现在 `/proc/<pid>/cmdline`、`ps`、Task Manager 等同机可读的
//! 进程信息里(对 sshpass `-p` 模式的修复)。

use crate::types::{AuthType, ConnectionStatus, SshServerInfo};
use futures_lite::io::AsyncWriteExt as _;
use std::borrow::Cow;
use std::process::Stdio;
use std::time::Duration;
use zeroize::Zeroizing;

pub fn build_ssh_args(server: &SshServerInfo) -> Vec<String> {
    let mut args: Vec<String> = vec!["ssh".into()];
    if server.port != 22 {
        args.push("-p".into());
        args.push(server.port.to_string());
    }
    if server.auth_type == AuthType::Key
        && let Some(path) = server.key_path.as_deref()
        && !path.is_empty()
    {
        args.push("-i".into());
        args.push(path.to_string());
    }
    let target = if server.username.is_empty() {
        server.host.clone()
    } else {
        format!("{}@{}", server.username, server.host)
    };
    args.push(target);
    args
}

pub fn build_ssh_command_line(server: &SshServerInfo) -> String {
    let args = build_ssh_args(server);
    args.iter()
        .map(|a| shell_escape::unix::escape(Cow::Borrowed(a.as_str())).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ConnectionTestResult {
    pub status: ConnectionStatus,
    pub latency_ms: Option<u64>,
    pub error_message: Option<String>,
}

pub async fn test_connection(
    server: &SshServerInfo,
    password: Option<Zeroizing<String>>,
) -> ConnectionTestResult {
    let start = instant::Instant::now();

    let result = match server.auth_type {
        AuthType::Key => test_key_auth(server).await,
        AuthType::Password => test_password_auth(server, password).await,
    };

    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(()) => ConnectionTestResult {
            status: ConnectionStatus::Online,
            latency_ms: Some(latency),
            error_message: None,
        },
        Err(e) => ConnectionTestResult {
            status: ConnectionStatus::Offline,
            latency_ms: Some(latency),
            error_message: Some(e),
        },
    }
}

async fn test_key_auth(server: &SshServerInfo) -> Result<(), String> {
    let args = build_ssh_args(server);
    let mut cmd_args = args.clone();
    cmd_args.push("-o".into());
    cmd_args.push("BatchMode=yes".into());
    cmd_args.push("-o".into());
    cmd_args.push("ConnectTimeout=5".into());
    cmd_args.push("-o".into());
    cmd_args.push("StrictHostKeyChecking=no".into());
    cmd_args.push("-o".into());
    cmd_args.push("LogLevel=ERROR".into());
    cmd_args.push("echo ok".into());

    match tokio::time::timeout(TEST_TIMEOUT, run_ssh_test(&cmd_args)).await {
        Ok(Ok(output)) => {
            // 严格匹配 `echo ok`,不放过 banner/motd 末尾恰好是 "ok" 的误判。
            if output.trim() == "ok" {
                Ok(())
            } else {
                Err(format!("Unexpected output: {}", output.trim()))
            }
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("Connection timeout".into()),
    }
}

async fn test_password_auth(
    server: &SshServerInfo,
    password: Option<Zeroizing<String>>,
) -> Result<(), String> {
    let password = password.ok_or("Password not provided")?;

    // 构造 ssh 命令参数。build_ssh_args 第一项是 "ssh" 程序名,我们用
    // Command::new("ssh") 显式派生,所以这里跳过头部。`BatchMode=no` 强制
    // ssh 走交互模式才能从 stdin 读密码;`NumberOfPasswordPrompts=1` 把
    // 重试窗口锁到 1 次;`PreferredAuthentications` 屏蔽公钥 / GSSAPI 等
    // 其他认证方式,避免我们误把公钥成功当成"密码对"。
    let mut cmd_args: Vec<String> = build_ssh_args(server)
        .into_iter()
        .skip(1)
        .collect();
    cmd_args.extend([
        "-o".into(),
        "BatchMode=no".into(),
        "-o".into(),
        "PreferredAuthentications=password,keyboard-interactive".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=1".into(),
        "-o".into(),
        "ConnectTimeout=5".into(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
        "echo ok".into(),
    ]);

    // 准备 stdin 缓冲:密码 + 换行。Zeroizing 包裹,作用域结束自动归零。
    let stdin_bytes = build_password_auth_stdin(&password);

    // 派生 ssh 子进程。stdin/stdout/stderr 全部 piped,这样可以写密码
    // 并在 wait 时读回响应。注意不能用 command::r#async::Command::output()
    // —— 它会把 stdin 强制设成 null。kill_on_drop(true) 保证下面 timeout
    // 命中时 child 被 drop 会自动 kill,不会留孤儿进程。
    let mut child = command::r#async::Command::new("ssh")
        .args(&cmd_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("启动 ssh 失败: {e}"))?;

    // 一次性写入密码 + \n。drop 后 ssh 端 stdin EOF,不会再读第二次。
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&stdin_bytes)
            .await
            .map_err(|e| format!("写入密码失败: {e}"))?;
    }

    // 带超时等子进程结束。async_process::Child::output() 会 collect
    // stdout/stderr 并返回 std::process::Output,不会消耗 stdin。
    // timeout 命中时 child 被 drop → kill_on_drop 自动 kill ssh。
    let output = match tokio::time::timeout(TEST_TIMEOUT, child.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("读取 ssh 输出失败: {e}")),
        Err(_) => return Err("Connection timeout".into()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 成功判定:严格匹配 `echo ok` 的输出。原先 `ends_with("ok")` 的兜底
    // 会让 banner / motd 末尾碰巧以 "ok" 结尾时误判为成功,这里去掉。
    if output.status.success() && stdout.trim() == "ok" {
        Ok(())
    } else if stderr.contains("Permission denied") || stderr.contains("Authentication failed") {
        Err("Authentication failed: wrong password".into())
    } else {
        Err(format!(
            "Unexpected output: stdout={} stderr={}",
            stdout.trim(),
            stderr.trim()
        ))
    }
}

/// 把密码编码成要写入 ssh stdin 的字节流:密码 UTF-8 + 换行。
/// 独立成纯函数,便于单测断言"stdin 包含密码字面量 + 换行"。
fn build_password_auth_stdin(password: &Zeroizing<String>) -> Zeroizing<Vec<u8>> {
    let mut v = Zeroizing::new(Vec::with_capacity(password.len() + 1));
    v.extend_from_slice(password.as_bytes());
    v.push(b'\n');
    v
}

async fn run_ssh_test(args: &[String]) -> Result<String, std::io::Error> {
    // 统一走 command::r#async 派生子进程,Windows 上会带 CREATE_NO_WINDOW,
    // 避免闪出控制台窗口(见 .clippy.toml 对 tokio::process::Command 的禁用)。
    let output = command::r#async::Command::new(&args[0])
        .args(&args[1..])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // 成功判定:进程退出码为 0,或远端 `echo ok` 的输出已回传(部分 sshpass
    // 警告会让退出码非零,但 stdout 里仍含 "ok")。
    if output.status.success() || stdout.contains("ok") {
        Ok(stdout)
    } else {
        Err(std::io::Error::other(stderr))
    }
}

#[cfg(test)]
#[path = "ssh_command_tests.rs"]
mod tests;
