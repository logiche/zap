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

    // 构造 ssh 命令参数。`build_password_auth_cmd_args` 集中处理所有
    // -o 选项,包括关键的 `PreferredAuthentications=password` 和
    // `KbdInteractiveAuthentication=no`(见该函数注释)。
    let cmd_args = build_password_auth_cmd_args(server);

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
    let stderr_trimmed = String::from_utf8_lossy(&output.stderr).trim().to_string();

    // 始终把 ssh 真实 stderr 落日志,即便成功也留痕,便于事后排查
    // "为什么 server 接受了 password 但 UI 报成功"的差异。
    if !stderr_trimmed.is_empty() {
        log::warn!("ssh test stderr: {stderr_trimmed}");
    }

    // 成功判定:严格匹配 `echo ok` 的输出。原先 `ends_with("ok")` 的兜底
    // 会让 banner / motd 末尾碰巧以 "ok" 结尾时误判为成功,这里去掉。
    if output.status.success() && stdout.trim() == "ok" {
        Ok(())
    } else if stderr_trimmed.contains("Permission denied")
        || stderr_trimmed.contains("Authentication failed")
    {
        // 错误信息带上精简 stderr(<= 200 字符),便于用户判断 server 端
        // 是没启 password、还是配置了 kbd-only AuthenticationMethods 等。
        let detail = if stderr_trimmed.is_empty() {
            String::new()
        } else {
            let snippet: String = stderr_trimmed.chars().take(200).collect();
            if stderr_trimmed.chars().count() > 200 {
                format!(" ({snippet}...)")
            } else {
                format!(" ({snippet})")
            }
        };
        Err(format!("Authentication failed: wrong password{detail}"))
    } else {
        Err(format!(
            "Unexpected output: stdout={} stderr={}",
            stdout.trim(),
            stderr_trimmed
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

/// 拼出 password 认证测试时给 ssh 子进程的完整 argv。
///
/// 与 `build_ssh_args` 不同:这里跳过首项 `"ssh"`(我们用
/// `Command::new("ssh")` 显式派生),追加测试用 `-o` 选项和 `echo ok` 远端命令。
///
/// 关键选项含义:
/// - `BatchMode=no`:允许 ssh 从 stdin 读密码(我们用 pipe 注入,不走 askpass)
/// - `PreferredAuthentications=password`:**只**声明想试 password,不带
///   `keyboard-interactive`。否则 server 端 PAM 在 password 之后会触发
///   kbd-interactive fallback,而我们的 stdin 在写完密码后已 EOF,kbd-int
///   子 prompt 拿不到响应,会逐项重试并触发 `pam_faildelay`(~2s/次),
///   累计 ~8-10s 顶满 `TEST_TIMEOUT`。
/// - `KbdInteractiveAuthentication=no`:客户端能力开关,直接禁掉整个 kbd-int
///   协议。光靠 `PreferredAuthentications` 不够——它只约束 password 子方法的
///   prompt 次数,kbd-int 仍可走;两个开关都设才是 defense in depth。
/// - `NumberOfPasswordPrompts=1`:password 子方法只允许 1 次重试。
/// - `ConnectTimeout=5`:单次 TCP 连接超时。
/// - `StrictHostKeyChecking=no`:不拦 known_hosts(测试场景下避免 host key
///   变化导致误报,真实终端连接走别的路径)。
/// - `LogLevel=ERROR`:抑制 host key 提示 / banner 等噪音。
///
/// `echo ok` 作为远端命令,严格匹配 stdout 判定成功(避免 banner / motd
/// 末尾恰好含 "ok" 的误判)。
///
/// author: logic
/// date: 2026-06-01
fn build_password_auth_cmd_args(server: &SshServerInfo) -> Vec<String> {
    let mut args: Vec<String> = build_ssh_args(server).into_iter().skip(1).collect();
    args.extend([
        "-o".into(),
        "BatchMode=no".into(),
        "-o".into(),
        "PreferredAuthentications=password".into(),
        "-o".into(),
        "KbdInteractiveAuthentication=no".into(),
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
    args
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
