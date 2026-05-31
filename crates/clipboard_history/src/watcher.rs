//! 剪贴板内容变化监听
//!
//! 通过 Windows 原生 `AddClipboardFormatListener` API 监听剪贴板变化，
//! 仅在内容实际改变时读取并通知主线程。
//!
//! author logic
//! date 2026-05-31

use std::ffi::c_void;
use std::mem::size_of;
use std::thread;

use arboard::Clipboard;
use async_channel::{Receiver, Sender, unbounded};
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// 剪贴板变化监听器
///
/// 后台线程创建隐藏窗口并注册 Windows 剪贴板变化通知，
/// 通过 `async_channel` 将新内容传递给主线程。
pub struct ClipboardWatcher {
    rx: Receiver<String>,
    /// 停止事件的原始句柄值（isize 可跨线程传递）
    stop_event: Option<isize>,
    _thread: Option<thread::JoinHandle<()>>,
}

impl ClipboardWatcher {
    /// 启动剪贴板监听
    pub fn start() -> anyhow::Result<Self> {
        let (tx, rx) = unbounded();

        let stop_event = unsafe { CreateEventW(None, true, false, None)? };
        let raw_handle = stop_event.0 as isize;

        let thread = thread::Builder::new()
            .name("Clipboard Watcher".into())
            .spawn(move || {
                let stop_event = HANDLE(raw_handle as *mut c_void);
                if let Err(e) = Self::run(tx, stop_event) {
                    log::error!("Clipboard watcher error: {e}");
                }
            })
            .map_err(|e| anyhow::anyhow!("failed to spawn clipboard watcher thread: {e}"))?;

        Ok(Self {
            rx,
            stop_event: Some(raw_handle),
            _thread: Some(thread),
        })
    }

    /// 返回事件接收端
    pub fn receiver(&self) -> Receiver<String> {
        self.rx.clone()
    }

    /// 停止监听器
    pub fn stop(&mut self) {
        if let Some(raw) = self.stop_event.take() {
            unsafe {
                let _ = SetEvent(HANDLE(raw as *mut c_void));
            }
        }
    }

    /// 后台线程主循环
    ///
    /// 创建隐藏窗口并注册剪贴板变化监听，使用 `MsgWaitForMultipleObjects`
    /// 同时等待 Windows 消息和停止信号。
    fn run(tx: Sender<String>, stop_event: HANDLE) -> anyhow::Result<()> {
        unsafe {
            let class_name = windows::core::w!("ZapClipboardWatcher");

            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(Self::wnd_proc),
                hInstance: GetModuleHandleW(None)?.into(),
                lpszClassName: class_name,
                ..Default::default()
            };

            RegisterClassExW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                windows::core::w!("ZapClipboardWatcher"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(wc.hInstance),
                None,
            )?;

            AddClipboardFormatListener(hwnd)?;

            let mut msg = MSG::default();
            loop {
                let result = MsgWaitForMultipleObjects(
                    Some(&[stop_event]),
                    false,
                    INFINITE,
                    QS_ALLINPUT,
                );

                if result == WAIT_OBJECT_0 {
                    break;
                }

                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_CLIPBOARDUPDATE {
                        Self::read_and_send(&tx);
                    }
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }

            let _ = RemoveClipboardFormatListener(hwnd);
        }

        Ok(())
    }

    /// 窗口过程（仅使用默认处理）
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// 读取当前剪贴板文本并发送到通道
    fn read_and_send(tx: &Sender<String>) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    let _ = tx.try_send(text);
                }
            }
        }
    }
}

impl Drop for ClipboardWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
