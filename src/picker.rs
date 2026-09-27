//! A floating picker stays usable even when the main image is only a few pixels wide.
use std::cell::Cell;
use windows::{
    core::w,
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};
thread_local! {
    static WINDOW: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static OWNER: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static LAST_POINTER: Cell<Option<isize>> = const { Cell::new(None) };
}

pub fn close() {
    WINDOW.with(|window| unsafe {
        let hwnd = window.replace(HWND::default());
        if !hwnd.0.is_null() {
            let _ = DestroyWindow(hwnd);
        }
    });
}

pub fn show(owner: HWND) {
    WINDOW.with(|window| unsafe {
        if !window.get().0.is_null() {
            return;
        }
        OWNER.with(|slot| slot.set(owner));
        LAST_POINTER.with(|slot| slot.set(None));
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSW {
            hInstance: instance,
            lpszClassName: w!("ImeRebornPicker"),
            lpfnWndProc: Some(window_proc),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&class);
        let mut bounds = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetWindowRect(owner, &mut bounds).is_err()
            || !GetMonitorInfoW(
                MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .as_bool()
        {
            return;
        }
        let work = monitor.rcWork;
        let width = 808.min(work.right - work.left);
        let height = 404.min(work.bottom - work.top);
        let x = ((bounds.left + bounds.right - width) / 2).clamp(work.left, work.right - width);
        let y = (bounds.top + (bounds.bottom - bounds.top) * 3 / 4 - height / 2)
            .clamp(work.top, work.bottom - height);
        if let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("ImeRebornPicker"),
            w!("Thumbnail browser"),
            WS_POPUP,
            x,
            y,
            width,
            height,
            owner,
            None,
            instance,
            None,
        ) {
            window.set(hwnd);
            refresh();
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    });
}

pub fn refresh() {
    WINDOW.with(|window| unsafe {
        let hwnd = window.get();
        if hwnd.0.is_null() {
            return;
        }
        let overlay = super::STATE
            .lock()
            .ok()
            .and_then(|state| state.thumbnails.clone());
        let Some(overlay) = overlay else {
            return;
        };
        let mut bounds = RECT::default();
        if GetWindowRect(hwnd, &mut bounds).is_err() {
            return;
        }
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        // Almost transparent background receives pointer input between cells.
        let mut pixels = vec![0; (width * height * 4) as usize];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 1;
        }
        overlay.render(&mut pixels, width, height);
        for pixel in pixels.chunks_exact_mut(4) {
            let alpha = pixel[3] as u16;
            for channel in &mut pixel[..3] {
                *channel = (*channel as u16 * alpha / 255) as u8;
            }
            pixel.swap(0, 2);
        }
        super::draw_layered(hwnd, width, height, &pixels, |_| {});
    });
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_MOUSEMOVE => {
            // Layered-window updates can synthesize a move at the same position.
            // Do not let a stationary pointer undo keyboard/wheel selection.
            if LAST_POINTER.with(|slot| slot.replace(Some(lparam.0)) != Some(lparam.0)) {
                super::hover_thumbnail(hwnd, lparam);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL | WM_MBUTTONDOWN | WM_KEYDOWN | WM_KEYUP => {
            OWNER.with(|owner| super::window_proc(owner.get(), message, wparam, lparam))
        }
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
