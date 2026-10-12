//! A floating picker stays usable even when the main image is only a few pixels wide.
use crate::thumbnails::ThumbnailScene;
use std::cell::{Cell, RefCell};
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
    static RENDERER: RefCell<Option<PickerRenderer>> = const { RefCell::new(None) };
}

const MOTION_TIMER: usize = 1;

/// Retain the DIB and DC across animation frames, as the original retains cell bitmaps.
struct PickerRenderer {
    scene: ThumbnailScene,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    timer_active: bool,
}

impl PickerRenderer {
    unsafe fn new(width: i32, height: i32) -> Option<Self> {
        let len = super::checked_rgba_len(width, height)?;
        let dc = CreateCompatibleDC(HDC::default());
        if dc.0.is_null() {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap =
            CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
        if bitmap.0.is_null() || bits.is_null() {
            if !bitmap.0.is_null() {
                let _ = DeleteObject(bitmap);
            }
            let _ = DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, bitmap);
        Some(Self {
            scene: ThumbnailScene::default(),
            width,
            height,
            pixels: vec![0; len],
            dc,
            bitmap,
            previous,
            bits: bits.cast(),
            timer_active: false,
        })
    }

    unsafe fn paint(&mut self, hwnd: HWND) {
        // A nearly transparent background still receives input between cells.
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 0, 1]);
        }
        self.scene.render(&mut self.pixels, self.width, self.height);
        for pixel in self.pixels.chunks_exact_mut(4) {
            let alpha = pixel[3] as u16;
            for channel in &mut pixel[..3] {
                *channel = (*channel as u16 * alpha / 255) as u8;
            }
            pixel.swap(0, 2);
        }
        std::ptr::copy_nonoverlapping(self.pixels.as_ptr(), self.bits, self.pixels.len());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            hwnd,
            HDC::default(),
            None,
            Some(&SIZE {
                cx: self.width,
                cy: self.height,
            }),
            self.dc,
            Some(&POINT::default()),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        if self.scene.is_animating() && !self.timer_active {
            self.timer_active = SetTimer(hwnd, MOTION_TIMER, 30, None) != 0;
        } else if !self.scene.is_animating() && self.timer_active {
            let _ = KillTimer(hwnd, MOTION_TIMER);
            self.timer_active = false;
        }
    }
}

impl Drop for PickerRenderer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

pub fn close() {
    WINDOW.with(|window| unsafe {
        let hwnd = window.replace(HWND::default());
        if !hwnd.0.is_null() {
            let _ = KillTimer(hwnd, MOTION_TIMER);
            let _ = DestroyWindow(hwnd);
        }
        RENDERER.with(|renderer| {
            renderer.borrow_mut().take();
        });
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
        RENDERER.with(|renderer| {
            let Ok(mut renderer) = renderer.try_borrow_mut() else {
                return;
            };
            if renderer
                .as_ref()
                .is_none_or(|old| old.width != width || old.height != height)
            {
                let _ = KillTimer(hwnd, MOTION_TIMER);
                *renderer = PickerRenderer::new(width, height);
            }
            if let Some(renderer) = renderer.as_mut() {
                renderer.scene.sync(&overlay, width, height);
                renderer.paint(hwnd);
            }
        });
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
                let x = (lparam.0 as u16) as i16 as i32;
                let y = ((lparam.0 >> 16) as u16) as i16 as i32;
                let index = RENDERER.with(|renderer| {
                    renderer.try_borrow().ok().and_then(|renderer| {
                        renderer
                            .as_ref()
                            .and_then(|renderer| renderer.scene.hit_test(x, y))
                    })
                });
                super::hover_thumbnail(index);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL | WM_MBUTTONDOWN | WM_KEYDOWN | WM_KEYUP => {
            OWNER.with(|owner| super::window_proc(owner.get(), message, wparam, lparam))
        }
        WM_TIMER if wparam.0 == MOTION_TIMER => {
            RENDERER.with(|renderer| {
                if let Ok(mut renderer) = renderer.try_borrow_mut() {
                    if let Some(renderer) = renderer.as_mut() {
                        renderer.scene.tick();
                        renderer.paint(hwnd);
                    }
                }
            });
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
