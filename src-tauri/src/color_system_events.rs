//! Hidden top-level window for display/settings broadcasts. Message-only windows
//! do not receive HWND_BROADCAST. The callback only invalidates adapter facts.
use raybend::color::system::ColorSystemError;
use std::sync::Arc;
use windows_sys::Win32::{Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::WindowsAndMessaging::*};

pub(super) struct DisplaySubscription {
    thread_id: u32,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for DisplaySubscription {
    fn drop(&mut self) {
        // SAFETY: our window loop has created its message queue before publishing
        // the id. WM_QUIT cleans up the window on its owning thread.
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0); }
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}
struct Context { invalidate: Arc<dyn Fn() + Send + Sync> }

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: WM_NCCREATE supplies CREATESTRUCTW. Context remains on the owning
    // thread until after DestroyWindow; userdata is cleared on WM_NCDESTROY.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        if matches!(message, WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DEVICECHANGE | WM_POWERBROADCAST) {
            let context = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Context;
            if let Some(context) = context.as_ref() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (context.invalidate)()));
            }
        }
        if message == WM_NCDESTROY { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0); }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

pub(super) fn subscribe(invalidate: Arc<dyn Fn() + Send + Sync>) -> Result<DisplaySubscription, ColorSystemError> {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new().name("color-system-events".into()).spawn(move || {
        let context = Context { invalidate };
        let class: Vec<u16> = "raybend.color.environment\0".encode_utf16().collect();
        // SAFETY: Win32 window lifetime and callback userdata belong exclusively
        // to this thread. No visible style, no parent, no borrowed foreign handle.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let descriptor = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..Default::default() };
            RegisterClassW(&descriptor);
            let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0,
                std::ptr::null_mut(), std::ptr::null_mut(), instance, &context as *const Context as *const std::ffi::c_void);
            if hwnd.is_null() { let _ = ready_tx.send(Err(std::io::Error::last_os_error().to_string())); return; }
            if ready_tx.send(Ok(GetCurrentThreadId())).is_err() { DestroyWindow(hwnd); return; }
            let mut message = MSG::default();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message); DispatchMessageW(&message);
            }
            DestroyWindow(hwnd);
        }
    }).map_err(|e| ColorSystemError::Unavailable(e.to_string()))?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(DisplaySubscription { thread_id, thread: Some(thread) }),
        error => { let _ = thread.join(); Err(ColorSystemError::Unavailable(format!("display event subscription: {error:?}"))) }
    }
}
