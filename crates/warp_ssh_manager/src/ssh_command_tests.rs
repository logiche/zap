//! `ssh_command` 单元测试。
//!
//! 按 `AGENTS.md §5.6` 拆出到独立文件,由 `ssh_command.rs` 末尾的 `#[path]` 引入。
//! 覆盖范围:
//! - `build_ssh_args` / `build_ssh_command_line` 参数构建
//! - `test_connection` 在缺密码 / 错认证类型时的错误路径
//! - `build_password_auth_stdin` 字节流构造(顺带覆盖 stdin 注入的关键安全路径)
//!
//! 注意:实际派生 ssh 子进程的端到端测试在 `app/src/ssh_manager/server_view.rs` 的
//! 集成测试 / 手测里覆盖 — 单测不做网络连接。
//!
//! author: logic
//! date: 2026-06-01

use super::*;
use zeroize::Zeroizing;

fn server() -> SshServerInfo {
    SshServerInfo {
        node_id: "n".into(),
        host: "1.2.3.4".into(),
        port: 22,
        username: "alice".into(),
        auth_type: AuthType::Password,
        key_path: None,
        startup_command: None,
        notes: None,
        last_connected_at: None,
    }
}

#[test]
fn default_port_omitted() {
    let s = server();
    assert_eq!(build_ssh_args(&s), vec!["ssh", "alice@1.2.3.4"]);
    // shell-escape 出于保守会把 user@host 用单引号引起来,这是合法且
    // shell-equivalent 的形式 — 不强求未引用版本。
    let line = build_ssh_command_line(&s);
    assert!(
        line == "ssh alice@1.2.3.4" || line == "ssh 'alice@1.2.3.4'",
        "unexpected: {line}"
    );
}

#[test]
fn custom_port_uses_dash_p() {
    let mut s = server();
    s.port = 2222;
    assert_eq!(
        build_ssh_args(&s),
        vec!["ssh", "-p", "2222", "alice@1.2.3.4"]
    );
}

#[test]
fn key_auth_emits_dash_i() {
    let mut s = server();
    s.auth_type = AuthType::Key;
    s.key_path = Some("/home/u/.ssh/id_ed25519".into());
    assert_eq!(
        build_ssh_args(&s),
        vec!["ssh", "-i", "/home/u/.ssh/id_ed25519", "alice@1.2.3.4"]
    );
}

#[test]
fn key_auth_without_path_is_skipped() {
    let mut s = server();
    s.auth_type = AuthType::Key;
    s.key_path = None;
    assert_eq!(build_ssh_args(&s), vec!["ssh", "alice@1.2.3.4"]);
}

#[test]
fn empty_username_yields_host_only() {
    let mut s = server();
    s.username = String::new();
    assert_eq!(build_ssh_args(&s), vec!["ssh", "1.2.3.4"]);
}

#[test]
fn shell_escapes_spaces_in_path() {
    let mut s = server();
    s.auth_type = AuthType::Key;
    s.key_path = Some("/path with spaces/id_rsa".into());
    let line = build_ssh_command_line(&s);
    assert!(
        line.contains("'/path with spaces/id_rsa'"),
        "actual: {line}"
    );
}

#[test]
fn test_connection_requires_password_for_password_auth() {
    let s = server();
    // test_connection 应该在没有密码时返回 Offline + 错误信息
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(test_connection(&s, None));
    assert_eq!(result.status, ConnectionStatus::Offline);
    assert!(result.error_message.unwrap().contains("Password not provided"));
}

#[test]
fn test_connection_key_auth_uses_batch_mode() {
    let mut s = server();
    s.auth_type = AuthType::Key;
    s.key_path = Some("/home/user/.ssh/id_rsa".into());
    // 对于密钥认证,应该走 BatchMode=yes 路径(由 run_ssh_test 携带);
    // 这里只验证 build_ssh_args 带了 -i 和 key_path。
    let args = build_ssh_args(&s);
    assert!(args.contains(&"-i".to_string()));
    assert!(args.contains(&"/home/user/.ssh/id_rsa".to_string()));
}

#[test]
fn connection_status_equality() {
    assert_eq!(ConnectionStatus::Online, ConnectionStatus::Online);
    assert_eq!(ConnectionStatus::Offline, ConnectionStatus::Offline);
    assert_eq!(ConnectionStatus::Unknown, ConnectionStatus::Unknown);
    assert_ne!(ConnectionStatus::Online, ConnectionStatus::Offline);
    assert_ne!(ConnectionStatus::Online, ConnectionStatus::Unknown);
    assert_ne!(ConnectionStatus::Offline, ConnectionStatus::Unknown);
}

// -------- 密码 stdin 注入安全相关 --------

/// 验证 `build_password_auth_stdin` 把密码 + 换行正确编码。
/// 这是密码泄露修复的关键:必须确认写进 ssh stdin 的字节流就是密码字面量 +
/// `\n`,而不是任何会让密码意外走 argv / 环境变量 / 临时文件的形态。
#[test]
fn build_password_auth_stdin_contains_password_with_newline() {
    let password: Zeroizing<String> = Zeroizing::new("s3cret-pass".into());
    let bytes = build_password_auth_stdin(&password);
    assert_eq!(&*bytes, b"s3cret-pass\n");
}

/// 边界:空密码仍要写一个 `\n`,这样 ssh 会立即拿到 EOF 并判定认证失败
/// (而不是卡在等待 prompt 的状态)。
#[test]
fn build_password_auth_stdin_empty_password_still_has_newline() {
    let password: Zeroizing<String> = Zeroizing::new(String::new());
    let bytes = build_password_auth_stdin(&password);
    assert_eq!(&*bytes, b"\n");
}

/// Unicode 密码:走 UTF-8 字节原样写入。
#[test]
fn build_password_auth_stdin_unicode_password() {
    let password: Zeroizing<String> = Zeroizing::new("密码🔐".into());
    let bytes = build_password_auth_stdin(&password);
    let mut expected = "密码🔐".as_bytes().to_vec();
    expected.push(b'\n');
    assert_eq!(&*bytes, expected.as_slice());
}

/// 回归:`build_ssh_args` 不应再带 `sshpass`,防止有人误把它加回 cmd_args
/// (Windows / macOS 默认无 sshpass,残留路径会立即 No such file or directory)。
#[test]
fn build_ssh_args_does_not_emit_sshpass() {
    let s = server();
    let args = build_ssh_args(&s);
    assert!(
        !args.iter().any(|a| a == "sshpass"),
        "build_ssh_args must not emit sshpass; got {args:?}"
    );
}
