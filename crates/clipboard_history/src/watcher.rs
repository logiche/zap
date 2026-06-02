//! 剪贴板内容变化监听
//!
//! 通过 Windows 原生 `AddClipboardFormatListener` API 监听剪贴板变化，
//! 仅在内容实际改变时读取并通知主线程。
//!
//! author logic
//! date 2026-05-31

#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "windows")]
use std::mem::size_of;
#[cfg(target_os = "windows")]
use std::thread;

#[cfg(target_os = "windows")]
use arboard::Clipboard;
use async_channel::{Receiver, unbounded};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;

// ==================== Windows 实现 ====================

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
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
        let raw_handle = self.stop_event.take();
        // 通知后台线程退出
        if let Some(raw) = raw_handle {
            unsafe {
                let _ = SetEvent(HANDLE(raw as *mut c_void));
            }
        }
        // 等待线程退出，确保不再使用句柄
        if let Some(thread) = self._thread.take() {
            let _ = thread.join();
        }
        // 线程已退出，安全关闭句柄
        if let Some(raw) = raw_handle {
            unsafe {
                let _ = CloseHandle(HANDLE(raw as *mut c_void));
            }
        }
    }

    /// 后台线程主循环
    ///
    /// 创建隐藏窗口并注册剪贴板变化监听，使用 `MsgWaitForMultipleObjects`
    /// 同时等待 Windows 消息和停止信号。
    fn run(tx: async_channel::Sender<String>, stop_event: HANDLE) -> anyhow::Result<()> {
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
    fn read_and_send(tx: &async_channel::Sender<String>) {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    let _ = tx.try_send(text);
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ClipboardWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

// ==================== 非 Windows 桩实现 ====================

#[cfg(not(target_os = "windows"))]
/// 剪贴板变化监听器（非 Windows 平台桩实现）
pub struct ClipboardWatcher {
    rx: Receiver<String>,
}

#[cfg(not(target_os = "windows"))]
impl ClipboardWatcher {
    /// 非 Windows 平台不支持剪贴板监听
    pub fn start() -> anyhow::Result<Self> {
        anyhow::bail!("clipboard watcher is only supported on Windows");
    }

    /// 返回事件接收端
    pub fn receiver(&self) -> Receiver<String> {
        self.rx.clone()
    }

    /// 停止监听器（空操作）
    pub fn stop(&mut self) {}
}

#[cfg(not(target_os = "windows"))]
impl Drop for ClipboardWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
