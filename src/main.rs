#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod decoder;
mod navigation;
mod thumbnails;

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use image::RgbaImage;
use navigation::{images_in_folder, same_path};
use thumbnails::ThumbnailOverlay;
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW,
            GetMonitorInfoW, MonitorFromWindow, SelectObject, SetBkMode, SetTextColor,
            AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
            CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_RIGHT,
            DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FF_DONTCARE, HBITMAP, HDC, MONITORINFO,
            MONITOR_DEFAULTTONEAREST, OUT_DEFAULT_PRECIS, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT},
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow,
                GetMessageW, GetWindowRect, IsWindow, KillTimer, LoadCursorW, MessageBoxW,
                PostMessageW, PostQuitMessage, RegisterClassW, SetTimer, SetWindowLongPtrW,
                SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, UpdateLayeredWindow,
                CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT,
                HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HWND_NOTOPMOST,
                HWND_TOPMOST, IDC_ARROW, MB_OK, MSG, SWP_NOMOVE, SWP_NOSIZE, SW_SHOW, ULW_ALPHA,
                WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT, WMSZ_TOP, WMSZ_TOPLEFT,
                WMSZ_TOPRIGHT, WM_CREATE, WM_DESTROY, WM_DROPFILES, WM_KEYDOWN, WM_KEYUP,
                WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCHITTEST, WM_SIZE, WM_SIZING, WM_TIMER, WNDCLASSW,
                WS_EX_APPWINDOW, WS_EX_LAYERED, WS_POPUP,
            },
        },
    },
};

const START_WIDTH: i32 = 420;
const START_HEIGHT: i32 = 260;
const KEY_F7: usize = 0x76;
const KEY_F8: usize = 0x77;
const KEY_ESCAPE: usize = 0x1b;
const KEY_HOME: usize = 0x24;
const KEY_END: usize = 0x23;
const KEY_SPACE: usize = 0x20;
const KEY_BACKSPACE: usize = 0x08;
const KEY_F1: usize = 0x70;
const KEY_B: usize = 0x42;
const KEY_R: usize = 0x52;
const KEY_SHIFT: usize = 0x10;
const KEY_ZERO: usize = 0x30;
const KEY_PLUS: usize = 0xbb;
const KEY_MINUS: usize = 0xbd;
const KEY_NUMPAD_PLUS: usize = 0x6b;
const KEY_NUMPAD_MINUS: usize = 0x6d;
const RESIZE_BORDER: i32 = 8;
const NAME_TIMER: usize = 1;
const ANIMATION_TIMER: usize = 2;
const ZOOM_RENDER_TIMER: usize = 3;
const WM_THUMBNAILS_READY: u32 = 0x8001;
const WM_IMAGE_READY: u32 = 0x8002;

#[derive(Clone, Copy)]
enum Background {
    Black,
    White,
    Checkerboard,
}

impl Background {
    fn next(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Checkerboard,
            Self::Checkerboard => Self::Black,
        }
    }
}

impl Default for Background {
    fn default() -> Self {
        Self::Black
    }
}

#[derive(Default)]
struct AppState {
    image: Option<Arc<RgbaImage>>,
    image_generation: u64,
    scaled_cache: Option<ScaledCache>,
    animation: Vec<decoder::DecodedFrame>,
    animation_index: usize,
    transparent: bool,
    topmost: bool,
    files: Vec<PathBuf>,
    current: usize,
    background: Background,
    overlay_name: Option<String>,
    thumbnails: Option<ThumbnailOverlay>,
    thumbnail_cache: Option<ThumbnailOverlay>,
    thumbnail_generation: u64,
    navigation_generation: u64,
    pending_image: Option<PendingImage>,
    zoom: f64,
    base_width: i32,
    base_height: i32,
    zoom_render_pending: bool,
    rotation_quarters: u8,
    original_width: u32,
    original_height: u32,
}

struct ScaledCache {
    generation: u64,
    width: i32,
    height: i32,
    zoom_milli: u32,
    rotation_quarters: u8,
    image: Arc<RgbaImage>,
}

struct PendingImage {
    generation: u64,
    decoded: decoder::DecodedImage,
}

static STATE: Mutex<AppState> = Mutex::new(AppState {
    image: None,
    image_generation: 0,
    scaled_cache: None,
    animation: Vec::new(),
    animation_index: 0,
    transparent: false,
    topmost: false,
    files: Vec::new(),
    current: 0,
    background: Background::Black,
    overlay_name: None,
    thumbnails: None,
    thumbnail_cache: None,
    thumbnail_generation: 0,
    navigation_generation: 0,
    pending_image: None,
    zoom: 1.0,
    base_width: START_WIDTH,
    base_height: START_HEIGHT,
    zoom_render_pending: false,
    rotation_quarters: 0,
    original_width: START_WIDTH as u32,
    original_height: START_HEIGHT as u32,
});

fn main() -> windows::core::Result<()> {
    let initial_path = env::args_os().nth(1).map(PathBuf::from);

    unsafe {
        let previously_focused = GetForegroundWindow();
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class_name = w!("ImpressionEyesRebornWindow");
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance,
            lpszClassName: class_name,
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        };
        RegisterClassW(&class);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_LAYERED,
            class_name,
            w!("Impression Eyes Reborn"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            START_WIDTH,
            START_HEIGHT,
            None,
            None,
            instance,
            None,
        )?;

        DragAcceptFiles(hwnd, true);
        center_on_focused_monitor(hwnd, previously_focused);
        if let Some(path) = initial_path {
            load_image(hwnd, path);
        } else {
            render(hwnd);
        }
        center_on_focused_monitor(hwnd, previously_focused);
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe fn center_on_focused_monitor(hwnd: HWND, focused: HWND) {
    let reference = if focused.0.is_null() { hwnd } else { focused };
    let monitor = MonitorFromWindow(reference, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let mut window = RECT::default();
    if !GetMonitorInfoW(monitor, &mut info).as_bool() || GetWindowRect(hwnd, &mut window).is_err() {
        return;
    }
    let width = window.right - window.left;
    let height = window.bottom - window.top;
    let x = info.rcWork.left + (info.rcWork.right - info.rcWork.left - width) / 2;
    let y = info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - height) / 2;
    let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create as isize);
            LRESULT(0)
        }
        WM_DROPFILES => {
            let drop = HDROP(wparam.0 as *mut _);
            let length = DragQueryFileW(drop, 0, None);
            if length > 0 {
                let mut buffer = vec![0u16; length as usize + 1];
                DragQueryFileW(drop, 0, Some(&mut buffer));
                buffer.truncate(length as usize);
                load_image(hwnd, PathBuf::from(OsString::from_wide(&buffer)));
            }
            DragFinish(drop);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_F7 => {
            if let Ok(mut state) = STATE.lock() {
                state.transparent = !state.transparent;
            }
            render(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_F8 => {
            if let Ok(mut state) = STATE.lock() {
                state.topmost = !state.topmost;
                let position = if state.topmost {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                };
                let _ = SetWindowPos(hwnd, position, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_ESCAPE => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_F1 => {
            show_shortcuts(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_B => {
            if let Ok(mut state) = STATE.lock() {
                state.background = state.background.next();
            }
            render(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_R => {
            rotate_image(hwnd, GetKeyState(VK_CONTROL.0 as i32) < 0);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_SHIFT => {
            show_thumbnail_overlay(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_PLUS || wparam.0 == KEY_NUMPAD_PLUS => {
            change_zoom(hwnd, 1.1);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_MINUS || wparam.0 == KEY_NUMPAD_MINUS => {
            change_zoom(hwnd, 1.0 / 1.1);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_ZERO => {
            reset_zoom(hwnd);
            LRESULT(0)
        }
        WM_KEYUP if wparam.0 == KEY_SHIFT => {
            commit_thumbnail_overlay(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            hover_thumbnail(hwnd, lparam);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_HOME => {
            select_image(hwnd, Selection::First);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_END => {
            select_image(hwnd, Selection::Last);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_SPACE => {
            select_image(hwnd, Selection::Relative(1));
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == KEY_BACKSPACE => {
            select_image(hwnd, Selection::Relative(-1));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            if GetKeyState(VK_CONTROL.0 as i32) < 0 {
                change_zoom(hwnd, 1.12_f64.powf(delta as f64 / 120.0));
            } else {
                select_image(hwnd, Selection::Relative(if delta > 0 { -1 } else { 1 }));
            }
            LRESULT(0)
        }
        WM_SIZE => {
            render(hwnd);
            LRESULT(0)
        }
        WM_SIZING => {
            constrain_aspect_ratio(wparam, lparam);
            LRESULT(1)
        }
        WM_TIMER if wparam.0 == NAME_TIMER => {
            let _ = KillTimer(hwnd, NAME_TIMER);
            if let Ok(mut state) = STATE.lock() {
                state.overlay_name = None;
            }
            render(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == ANIMATION_TIMER => {
            advance_animation(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == ZOOM_RENDER_TIMER => {
            let _ = KillTimer(hwnd, ZOOM_RENDER_TIMER);
            if let Ok(mut state) = STATE.lock() {
                state.zoom_render_pending = false;
            }
            render(hwnd);
            LRESULT(0)
        }
        WM_THUMBNAILS_READY => {
            if GetKeyState(VK_SHIFT.0 as i32) < 0 {
                show_thumbnail_overlay(hwnd);
            }
            LRESULT(0)
        }
        WM_IMAGE_READY => {
            apply_pending_image(hwnd);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let thumbnails_visible = STATE
                .lock()
                .map(|state| state.thumbnails.is_some())
                .unwrap_or(false);
            if thumbnails_visible {
                LRESULT(HTCLIENT as isize)
            } else {
                resize_hit_test(hwnd, lparam)
            }
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn load_image(hwnd: HWND, path: PathBuf) {
    let already_loaded = STATE
        .lock()
        .map(|mut state| {
            let same = state
                .files
                .get(state.current)
                .map(|current| same_path(current, &path))
                .unwrap_or(false);
            if same {
                state.thumbnails = None;
            }
            same
        })
        .unwrap_or(false);
    if already_loaded {
        unsafe { render(hwnd) };
        return;
    }
    unsafe {
        let _ = KillTimer(hwnd, ANIMATION_TIMER);
    }
    let files = images_in_folder(&path);
    let current = files
        .iter()
        .position(|item| same_path(item, &path))
        .unwrap_or(0);
    let setup = STATE.lock().ok().map(|mut state| {
        let preview = state
            .thumbnail_cache
            .as_ref()
            .and_then(|cache| cache.preview_for_path(&path));
        state.navigation_generation = state.navigation_generation.wrapping_add(1);
        state.thumbnail_generation = state.thumbnail_generation.wrapping_add(1);
        state.pending_image = None;
        state.animation.clear();
        state.animation_index = 0;
        state.zoom = 1.0;
        state.rotation_quarters = 0;
        state.files = files;
        state.current = current;
        state.overlay_name = display_name(&path);
        state.thumbnails = None;
        if let Some(preview) = &preview {
            state.image = Some(preview.image.clone());
            state.original_width = preview.original_width;
            state.original_height = preview.original_height;
            state.image_generation = state.image_generation.wrapping_add(1);
            state.scaled_cache = None;
        }
        (
            state.navigation_generation,
            state.thumbnail_generation,
            state.files.clone(),
            preview,
        )
    });
    let Some((image_generation, thumbnail_generation, files, preview)) = setup else {
        return;
    };

    if let Some(preview) = preview {
        unsafe {
            let (width, height) =
                fit_to_monitor(hwnd, preview.original_width, preview.original_height);
            if let Ok(mut state) = STATE.lock() {
                state.base_width = width;
                state.base_height = height;
            }
            resize_around_center(hwnd, width, height);
        }
    }
    unsafe {
        announce_file(hwnd, &path);
        render(hwnd);
    }
    // Thumbnail decoding and the full-resolution decode deliberately start as
    // independent jobs. Neither can hold up the UI thread or the other job.
    preload_thumbnails(hwnd, files, current, thumbnail_generation);
    decode_selected_image(hwnd, path, image_generation);
}

unsafe fn resize_hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let x = (lparam.0 as u16) as i16 as i32;
    let y = ((lparam.0 >> 16) as u16) as i16 as i32;
    let mut bounds = RECT::default();
    if GetWindowRect(hwnd, &mut bounds).is_err() {
        return LRESULT(HTCAPTION as isize);
    }
    let left = x < bounds.left + RESIZE_BORDER;
    let right = x >= bounds.right - RESIZE_BORDER;
    let top = y < bounds.top + RESIZE_BORDER;
    let bottom = y >= bounds.bottom - RESIZE_BORDER;
    let hit = match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT,
        (_, true, true, _) => HTTOPRIGHT,
        (true, _, _, true) => HTBOTTOMLEFT,
        (_, true, _, true) => HTBOTTOMRIGHT,
        (true, _, _, _) => HTLEFT,
        (_, true, _, _) => HTRIGHT,
        (_, _, true, _) => HTTOP,
        (_, _, _, true) => HTBOTTOM,
        _ => HTCAPTION,
    };
    LRESULT(hit as isize)
}

unsafe fn show_shortcuts(hwnd: HWND) {
    let _ = MessageBoxW(
        hwnd,
        w!("Drop image: Open\nMouse wheel / Space / Backspace: Browse\nCtrl + mouse wheel or +/-: Zoom\n0: Reset zoom\nR: Rotate clockwise\nCtrl+R: Rotate counter-clockwise\nHome / End: First / last\nHold Shift: Thumbnail browser\nDrag: Move window\nEdges and corners: Resize\nCtrl + resize: Freeform stretch\nB: Cycle background\nF7: Desktop transparency\nF8: Always on top\nF1: Show this help\nEscape: Close"),
        w!("Impression Eyes Reborn shortcuts"),
        MB_OK,
    );
}

fn show_thumbnail_overlay(hwnd: HWND) {
    let activated = if let Ok(mut state) = STATE.lock() {
        let refreshed = match (state.thumbnail_cache.clone(), state.thumbnails.as_ref()) {
            (Some(cache), Some(previous)) => Some(cache.preserving_hover(previous)),
            (cache, None) => cache,
            (None, Some(_)) => state.thumbnails.clone(),
        };
        state.thumbnails = refreshed;
        state.thumbnails.is_some()
    } else {
        false
    };
    if activated {
        unsafe { render(hwnd) };
    }
}

fn preload_thumbnails(hwnd: HWND, files: Vec<PathBuf>, current: usize, generation: u64) {
    let hwnd_value = hwnd.0 as isize;
    std::thread::spawn(move || {
        let still_current = STATE
            .lock()
            .map(|state| state.thumbnail_generation == generation)
            .unwrap_or(false);
        if !still_current {
            return;
        }
        let mut last_notification = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(50))
            .unwrap_or_else(std::time::Instant::now);
        let publish = |overlay: ThumbnailOverlay| {
            let accepted = STATE
                .lock()
                .map(|mut state| {
                    if state.thumbnail_generation != generation {
                        return false;
                    }
                    state.thumbnail_cache = Some(overlay);
                    true
                })
                .unwrap_or(false);
            if accepted && last_notification.elapsed() >= std::time::Duration::from_millis(50) {
                unsafe {
                    let hwnd = HWND(hwnd_value as *mut _);
                    if IsWindow(hwnd).as_bool() {
                        let _ = PostMessageW(hwnd, WM_THUMBNAILS_READY, WPARAM(0), LPARAM(0));
                    }
                }
                last_notification = std::time::Instant::now();
            }
            accepted
        };
        let _ = ThumbnailOverlay::build_progressive(&files, current, publish);
        let still_current = STATE
            .lock()
            .map(|state| state.thumbnail_generation == generation)
            .unwrap_or(false);
        if still_current {
            unsafe {
                let hwnd = HWND(hwnd_value as *mut _);
                if IsWindow(hwnd).as_bool() {
                    let _ = PostMessageW(hwnd, WM_THUMBNAILS_READY, WPARAM(0), LPARAM(0));
                }
            }
        }
    });
}

fn hover_thumbnail(hwnd: HWND, lparam: LPARAM) {
    let x = (lparam.0 as u16) as i16 as i32;
    let y = ((lparam.0 >> 16) as u16) as i16 as i32;
    let mut bounds = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut bounds) }.is_err() {
        return;
    }
    let changed = STATE
        .lock()
        .ok()
        .and_then(|mut state| {
            state.thumbnails.as_mut().map(|overlay| {
                overlay.hover(x, y, bounds.right - bounds.left, bounds.bottom - bounds.top)
            })
        })
        .unwrap_or(false);
    if changed {
        unsafe { render(hwnd) };
    }
}

fn commit_thumbnail_overlay(hwnd: HWND) {
    let selection = STATE.lock().ok().map(|mut state| {
        let selected = state
            .thumbnails
            .as_ref()
            .and_then(ThumbnailOverlay::selected_path);
        let current = state.files.get(state.current).cloned();
        state.thumbnails = None;
        (selected, current)
    });
    match selection {
        Some((Some(selected), Some(current))) if !same_path(&selected, &current) => {
            load_image(hwnd, selected);
        }
        _ => unsafe { render(hwnd) },
    }
}

unsafe fn constrain_aspect_ratio(wparam: WPARAM, lparam: LPARAM) {
    if GetKeyState(VK_CONTROL.0 as i32) < 0 {
        return;
    }
    let ratio = {
        let Ok(state) = STATE.lock() else { return };
        let Some(image) = &state.image else { return };
        image.width() as f64 / image.height().max(1) as f64
    };
    let bounds = &mut *(lparam.0 as *mut RECT);
    let width = (bounds.right - bounds.left).max(1);
    let height = (bounds.bottom - bounds.top).max(1);
    let adjust_height = matches!(
        wparam.0 as u32,
        WMSZ_LEFT | WMSZ_RIGHT | WMSZ_TOPLEFT | WMSZ_TOPRIGHT | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT
    );
    if adjust_height {
        let new_height = (width as f64 / ratio).round() as i32;
        if matches!(wparam.0 as u32, WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT) {
            bounds.top = bounds.bottom - new_height;
        } else {
            bounds.bottom = bounds.top + new_height;
        }
    } else {
        let new_width = (height as f64 * ratio).round() as i32;
        if wparam.0 as u32 == WMSZ_LEFT {
            bounds.left = bounds.right - new_width;
        } else {
            bounds.right = bounds.left + new_width;
        }
    }
}

enum Selection {
    First,
    Last,
    Relative(isize),
}

fn change_zoom(hwnd: HWND, factor: f64) {
    let current = STATE.lock().map(|state| state.zoom).unwrap_or(1.0);
    set_zoom(hwnd, current * factor);
}

fn reset_zoom(hwnd: HWND) {
    set_zoom(hwnd, 1.0);
}

fn rotate_image(hwnd: HWND, counter_clockwise: bool) {
    let rotation = STATE.lock().ok().and_then(|mut state| {
        state.image.as_ref()?;
        let step = if counter_clockwise { 3 } else { 1 };
        state.rotation_quarters = (state.rotation_quarters + step) % 4;
        state.zoom = 1.0;
        state.scaled_cache = None;
        state.overlay_name = state
            .files
            .get(state.current)
            .and_then(|path| display_name(path));
        let (width, height) = oriented_dimensions(
            state.original_width,
            state.original_height,
            state.rotation_quarters,
        );
        Some((width, height))
    });
    let Some((image_width, image_height)) = rotation else {
        return;
    };
    unsafe {
        let (width, height) = fit_to_monitor(hwnd, image_width, image_height);
        if let Ok(mut state) = STATE.lock() {
            state.base_width = width;
            state.base_height = height;
        }
        resize_around_center(hwnd, width, height);
        let _ = KillTimer(hwnd, NAME_TIMER);
        SetTimer(hwnd, NAME_TIMER, 1800, None);
        render(hwnd);
    }
}

fn oriented_dimensions(width: u32, height: u32, rotation_quarters: u8) -> (u32, u32) {
    if rotation_quarters % 2 == 0 {
        (width, height)
    } else {
        (height, width)
    }
}

fn set_zoom(hwnd: HWND, requested: f64) {
    let zoom = requested.clamp(0.1, 8.0);
    let should_schedule = STATE.lock().ok().and_then(|mut state| {
        state.image.as_ref()?;
        state.zoom = zoom;
        state.scaled_cache = None;
        state.overlay_name = state
            .files
            .get(state.current)
            .and_then(|path| display_name(path));
        let should_schedule = !state.zoom_render_pending;
        state.zoom_render_pending = true;
        Some(should_schedule)
    });
    if let Some(should_schedule) = should_schedule {
        unsafe {
            let _ = KillTimer(hwnd, NAME_TIMER);
            SetTimer(hwnd, NAME_TIMER, 1800, None);
            if should_schedule {
                SetTimer(hwnd, ZOOM_RENDER_TIMER, 16, None);
            }
        }
    }
}

fn select_image(hwnd: HWND, selection: Selection) {
    let (path, generation, preview, thumbnail_preload) = {
        let Ok(mut state) = STATE.lock() else { return };
        if state.files.len() < 2 {
            return;
        }
        state.current = match selection {
            Selection::First => 0,
            Selection::Last => state.files.len() - 1,
            Selection::Relative(direction) => {
                let count = state.files.len() as isize;
                (state.current as isize + direction).rem_euclid(count) as usize
            }
        };
        let path = state.files[state.current].clone();
        state.navigation_generation = state.navigation_generation.wrapping_add(1);
        state.pending_image = None;
        state.animation.clear();
        state.animation_index = 0;
        state.zoom = 1.0;
        state.rotation_quarters = 0;
        state.overlay_name = display_name(&path);
        state.thumbnails = None;
        let preview = state
            .thumbnail_cache
            .as_ref()
            .and_then(|cache| cache.preview_for_path(&path));
        if let Some(preview) = &preview {
            state.image = Some(preview.image.clone());
            state.original_width = preview.original_width;
            state.original_height = preview.original_height;
            state.image_generation = state.image_generation.wrapping_add(1);
            state.scaled_cache = None;
        }
        let thumbnail_preload = if preview.is_none() {
            state.thumbnail_generation = state.thumbnail_generation.wrapping_add(1);
            Some((
                state.files.clone(),
                state.current,
                state.thumbnail_generation,
            ))
        } else {
            None
        };
        (
            path,
            state.navigation_generation,
            preview,
            thumbnail_preload,
        )
    };

    unsafe {
        let _ = KillTimer(hwnd, ANIMATION_TIMER);
    }
    if let Some(preview) = &preview {
        unsafe {
            let (width, height) =
                fit_to_monitor(hwnd, preview.original_width, preview.original_height);
            if let Ok(mut state) = STATE.lock() {
                if state.navigation_generation == generation {
                    state.base_width = width;
                    state.base_height = height;
                }
            }
            resize_around_center(hwnd, width, height);
            announce_file(hwnd, &path);
            render(hwnd);
        }
    }
    if let Some((files, current, thumbnail_generation)) = thumbnail_preload {
        preload_thumbnails(hwnd, files, current, thumbnail_generation);
    }
    decode_selected_image(hwnd, path, generation);
}

fn decode_selected_image(hwnd: HWND, path: PathBuf, generation: u64) {
    let hwnd_value = hwnd.0 as isize;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(35));
        let still_current = STATE
            .lock()
            .map(|state| state.navigation_generation == generation)
            .unwrap_or(false);
        if !still_current {
            return;
        }
        let Ok(decoded) = decoder::load_animated(&path) else {
            return;
        };
        let accepted = STATE
            .lock()
            .map(|mut state| {
                if state.navigation_generation != generation {
                    return false;
                }
                state.pending_image = Some(PendingImage {
                    generation,
                    decoded,
                });
                true
            })
            .unwrap_or(false);
        if accepted {
            unsafe {
                let hwnd = HWND(hwnd_value as *mut _);
                if IsWindow(hwnd).as_bool() {
                    let _ = PostMessageW(hwnd, WM_IMAGE_READY, WPARAM(0), LPARAM(0));
                }
            }
        }
    });
}

fn apply_pending_image(hwnd: HWND) {
    let animation = STATE.lock().ok().and_then(|mut state| {
        let pending = state.pending_image.take()?;
        if pending.generation != state.navigation_generation {
            return None;
        }
        let first_image = pending.decoded.first().image.clone();
        let image_width = first_image.width();
        let image_height = first_image.height();
        let first_delay = pending.decoded.first().delay_ms;
        let animated = pending.decoded.frames.len() > 1;
        let (oriented_width, oriented_height) =
            oriented_dimensions(image_width, image_height, state.rotation_quarters);
        let (base_width, base_height) =
            unsafe { fit_to_monitor(hwnd, oriented_width, oriented_height) };
        state.image = Some(first_image);
        state.image_generation = state.image_generation.wrapping_add(1);
        state.scaled_cache = None;
        state.animation = pending.decoded.frames;
        state.animation_index = 0;
        state.original_width = image_width;
        state.original_height = image_height;
        state.base_width = base_width;
        state.base_height = base_height;
        Some((animated, first_delay, base_width, base_height))
    });
    if let Some((animated, first_delay, width, height)) = animation {
        unsafe {
            resize_around_center(hwnd, width, height);
            render(hwnd);
            schedule_animation(hwnd, animated, first_delay);
        }
    }
}

unsafe fn resize_around_center(hwnd: HWND, width: i32, height: i32) {
    let mut current = RECT::default();
    if GetWindowRect(hwnd, &mut current).is_err() {
        let _ = SetWindowPos(hwnd, None, 0, 0, width, height, SWP_NOMOVE);
        return;
    }
    let center_x = current.left + (current.right - current.left) / 2;
    let center_y = current.top + (current.bottom - current.top) / 2;
    let x = center_x - width / 2;
    let y = center_y - height / 2;
    let _ = SetWindowPos(hwnd, None, x, y, width, height, Default::default());
}

fn display_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

unsafe fn schedule_animation(hwnd: HWND, animated: bool, delay_ms: u32) {
    let _ = KillTimer(hwnd, ANIMATION_TIMER);
    if animated {
        SetTimer(hwnd, ANIMATION_TIMER, delay_ms, None);
    }
}

fn advance_animation(hwnd: HWND) {
    let next_delay = STATE.lock().ok().and_then(|mut state| {
        if state.animation.len() < 2 {
            return None;
        }
        state.animation_index = (state.animation_index + 1) % state.animation.len();
        let frame = &state.animation[state.animation_index];
        let image = frame.image.clone();
        let delay = frame.delay_ms;
        state.image = Some(image);
        state.image_generation = state.image_generation.wrapping_add(1);
        state.scaled_cache = None;
        Some(delay)
    });
    if let Some(delay) = next_delay {
        unsafe {
            render(hwnd);
            schedule_animation(hwnd, true, delay);
        }
    }
}

unsafe fn announce_file(hwnd: HWND, path: &Path) {
    use std::os::windows::ffi::OsStrExt;

    let title = format!(
        "{} — Impression Eyes Reborn",
        display_name(path).unwrap_or_else(|| "Image".to_string())
    );
    let mut title_wide: Vec<u16> = std::ffi::OsStr::new(&title).encode_wide().collect();
    title_wide.push(0);
    let _ = SetWindowTextW(hwnd, PCWSTR(title_wide.as_ptr()));
    let _ = KillTimer(hwnd, NAME_TIMER);
    SetTimer(hwnd, NAME_TIMER, 1800, None);
}

unsafe fn fit_to_monitor(hwnd: HWND, image_width: u32, image_height: u32) -> (i32, i32) {
    let native_width = image_width.min(i32::MAX as u32).max(1) as i32;
    let native_height = image_height.min(i32::MAX as u32).max(1) as i32;
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return (native_width, native_height);
    }
    let available_width = (info.rcWork.right - info.rcWork.left).max(1);
    let available_height = (info.rcWork.bottom - info.rcWork.top).max(1);
    let scale = (available_width as f64 / native_width as f64)
        .min(available_height as f64 / native_height as f64)
        .min(1.0);
    (
        (native_width as f64 * scale).round().max(1.0) as i32,
        (native_height as f64 * scale).round().max(1.0) as i32,
    )
}

unsafe fn render(hwnd: HWND) {
    let (
        image,
        generation,
        transparent,
        background,
        overlay_name,
        zoom,
        rotation_quarters,
        thumbnails,
    ) = {
        let Ok(state) = STATE.lock() else { return };
        (
            state.image.clone(),
            state.image_generation,
            state.transparent,
            state.background,
            state.overlay_name.clone(),
            state.zoom,
            state.rotation_quarters,
            state.thumbnails.clone(),
        )
    };
    let mut bounds = RECT::default();
    if GetWindowRect(hwnd, &mut bounds).is_err() {
        return;
    }
    let width = (bounds.right - bounds.left).max(1);
    let height = (bounds.bottom - bounds.top).max(1);
    if checked_rgba_len(width, height).is_none() {
        return;
    }
    let show_startup_help = image.is_none();
    let mut pixels = match image {
        Some(image) => scaled_image(&image, generation, width, height, zoom, rotation_quarters)
            .as_raw()
            .clone(),
        None => {
            let Some(pixels) = startup_pixels(width, height) else {
                return;
            };
            pixels
        }
    };

    let display_label =
        overlay_name.map(|name| format!("{name} ({}%)", (zoom * 100.0).round() as u32));
    if let Some(name) = &display_label {
        let label_width = filename_label_width(name, width);
        let label_left = width - label_width;
        let bar_height = height.min(24);
        for y in 0..bar_height {
            for x in label_left..width {
                let offset = ((y * width + x) * 4) as usize;
                pixels[offset] = (pixels[offset] as u16 * 42 / 100) as u8;
                pixels[offset + 1] = (pixels[offset + 1] as u16 * 42 / 100) as u8;
                pixels[offset + 2] = (pixels[offset + 2] as u16 * 42 / 100) as u8;
                pixels[offset + 3] = 255;
            }
        }
    }
    if let Some(overlay) = thumbnails {
        overlay.render(&mut pixels, width, height);
    }

    // DIB sections use BGRA. Layered windows also require premultiplied alpha.
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let source_alpha = pixel[3] as u16;
        if transparent {
            pixel[0] = (pixel[0] as u16 * source_alpha / 255) as u8;
            pixel[1] = (pixel[1] as u16 * source_alpha / 255) as u8;
            pixel[2] = (pixel[2] as u16 * source_alpha / 255) as u8;
            pixel[3] = source_alpha as u8;
        } else {
            let x = index as i32 % width;
            let y = index as i32 / width;
            let background = background_rgb(background, x, y);
            let inverse = 255 - source_alpha;
            pixel[0] =
                ((pixel[0] as u16 * source_alpha + background[0] as u16 * inverse) / 255) as u8;
            pixel[1] =
                ((pixel[1] as u16 * source_alpha + background[1] as u16 * inverse) / 255) as u8;
            pixel[2] =
                ((pixel[2] as u16 * source_alpha + background[2] as u16 * inverse) / 255) as u8;
            pixel[3] = 255;
        }
        pixel.swap(0, 2);
    }

    let screen = HDC::default();
    let memory_dc = CreateCompatibleDC(screen);
    let bitmap_info = BITMAPINFO {
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
    let bitmap: HBITMAP =
        CreateDIBSection(memory_dc, &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
            .unwrap_or_default();
    if bitmap.0.is_null() || bits.is_null() {
        let _ = DeleteDC(memory_dc);
        return;
    }
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
    let old = SelectObject(memory_dc, bitmap);
    if show_startup_help {
        draw_startup_helper(memory_dc, width, height);
    } else if let Some(name) = display_label {
        draw_filename(memory_dc, width, &name);
    }
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT::default();
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let _ = UpdateLayeredWindow(
        hwnd,
        screen,
        None,
        Some(&size),
        memory_dc,
        Some(&source),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    SelectObject(memory_dc, old);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(memory_dc);
}

fn scaled_image(
    image: &Arc<RgbaImage>,
    generation: u64,
    width: i32,
    height: i32,
    zoom: f64,
    rotation_quarters: u8,
) -> Arc<RgbaImage> {
    let zoom_milli = (zoom * 1000.0).round() as u32;
    if let Ok(state) = STATE.lock() {
        if let Some(cache) = &state.scaled_cache {
            if cache.generation == generation
                && cache.width == width
                && cache.height == height
                && cache.zoom_milli == zoom_milli
                && cache.rotation_quarters == rotation_quarters
            {
                return cache.image.clone();
            }
        }
    }
    let rotated = match rotation_quarters % 4 {
        0 => None,
        1 => Some(image::imageops::rotate90(image.as_ref())),
        2 => Some(image::imageops::rotate180(image.as_ref())),
        3 => Some(image::imageops::rotate270(image.as_ref())),
        _ => unreachable!(),
    };
    let source = rotated.as_ref().unwrap_or(image.as_ref());
    let scaled = Arc::new(scale_into_viewport(
        source,
        width as u32,
        height as u32,
        zoom,
    ));
    if let Ok(mut state) = STATE.lock() {
        if state.image_generation == generation {
            state.scaled_cache = Some(ScaledCache {
                generation,
                width,
                height,
                zoom_milli,
                rotation_quarters,
                image: scaled.clone(),
            });
        }
    }
    scaled
}

fn scale_into_viewport(image: &RgbaImage, width: u32, height: u32, zoom: f64) -> RgbaImage {
    let image_width = image.width().max(1) as f64;
    let image_height = image.height().max(1) as f64;
    let scale = (width as f64 / image_width).min(height as f64 / image_height) * zoom;
    let drawn_width = (image_width * scale).max(1.0);
    let drawn_height = (image_height * scale).max(1.0);
    let left = (width as f64 - drawn_width) / 2.0;
    let top = (height as f64 - drawn_height) / 2.0;
    let visible_left = left.max(0.0);
    let visible_top = top.max(0.0);
    let visible_right = (left + drawn_width).min(width as f64);
    let visible_bottom = (top + drawn_height).min(height as f64);
    let destination_width = (visible_right - visible_left).round().max(1.0) as u32;
    let destination_height = (visible_bottom - visible_top).round().max(1.0) as u32;

    let source_left = ((visible_left - left) / scale).floor().max(0.0) as u32;
    let source_top = ((visible_top - top) / scale).floor().max(0.0) as u32;
    let source_right = ((visible_right - left) / scale).ceil().min(image_width) as u32;
    let source_bottom = ((visible_bottom - top) / scale).ceil().min(image_height) as u32;
    let source_width = source_right.saturating_sub(source_left).max(1);
    let source_height = source_bottom.saturating_sub(source_top).max(1);
    let resized = if source_left == 0
        && source_top == 0
        && source_width == image.width()
        && source_height == image.height()
    {
        image::imageops::resize(
            image,
            destination_width,
            destination_height,
            image::imageops::FilterType::Triangle,
        )
    } else {
        let crop =
            image::imageops::crop_imm(image, source_left, source_top, source_width, source_height);
        image::imageops::resize(
            &crop.to_image(),
            destination_width,
            destination_height,
            image::imageops::FilterType::Triangle,
        )
    };
    let mut viewport = RgbaImage::new(width, height);
    image::imageops::overlay(
        &mut viewport,
        &resized,
        visible_left.round() as i64,
        visible_top.round() as i64,
    );
    viewport
}

fn checked_rgba_len(width: i32, height: i32) -> Option<usize> {
    const MAX_RENDER_BYTES: usize = 256 * 1024 * 1024;
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    width
        .checked_mul(height)?
        .checked_mul(4)
        .filter(|length| *length <= MAX_RENDER_BYTES)
}

fn filename_label_width(name: &str, viewport_width: i32) -> i32 {
    let desired = (name.chars().count() as i32)
        .saturating_mul(7)
        .saturating_add(18)
        .max(70);
    desired.min(viewport_width.max(1))
}

fn background_rgb(background: Background, x: i32, y: i32) -> [u8; 3] {
    match background {
        Background::Black => [0, 0, 0],
        Background::White => [255, 255, 255],
        Background::Checkerboard => {
            if (x / 12 + y / 12) % 2 == 0 {
                [54, 54, 58]
            } else {
                [92, 92, 98]
            }
        }
    }
}

fn startup_pixels(width: i32, height: i32) -> Option<Vec<u8>> {
    let mut pixels = vec![0u8; checked_rgba_len(width, height)?];
    for y in 0..height {
        let shade = 24 + (y * 8 / height.max(1)) as u8;
        for x in 0..width {
            let offset = ((y * width + x) * 4) as usize;
            pixels[offset] = shade.saturating_sub(2);
            pixels[offset + 1] = shade;
            pixels[offset + 2] = shade.saturating_add(4);
            pixels[offset + 3] = 255;
        }
    }

    // A restrained dashed drop-zone outline, inset enough to leave resize handles clear.
    let left = 28;
    let top = 28;
    let right = width - 29;
    let bottom = height - 29;
    let border = [72, 82, 96, 255];
    for x in (left + 14..right - 14).step_by(12) {
        paint_square(&mut pixels, width, height, x, top, border);
        paint_square(&mut pixels, width, height, x, bottom, border);
    }
    for y in (top + 14..bottom - 14).step_by(12) {
        paint_square(&mut pixels, width, height, left, y, border);
        paint_square(&mut pixels, width, height, right, y, border);
    }
    Some(pixels)
}

fn paint_square(pixels: &mut [u8], width: i32, height: i32, x: i32, y: i32, color: [u8; 4]) {
    for offset_y in 0..2 {
        for offset_x in 0..2 {
            let px = x + offset_x;
            let py = y + offset_y;
            if px >= 0 && py >= 0 && px < width && py < height {
                let offset = ((py * width + px) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
}

unsafe fn draw_startup_helper(dc: HDC, width: i32, height: i32) {
    let _ = SetBkMode(dc, TRANSPARENT);
    let title_font = CreateFontW(
        -24,
        0,
        0,
        0,
        600,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        0,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Segoe UI"),
    );
    let body_font = CreateFontW(
        -15,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        0,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Segoe UI"),
    );

    let old_font = SelectObject(dc, title_font);
    let _ = SetTextColor(dc, COLORREF(0x00f5f3f0));
    draw_centered_text(dc, "Drop an image", 74, 116, width);

    let _ = SelectObject(dc, body_font);
    let _ = SetTextColor(dc, COLORREF(0x00aaa29a));
    draw_centered_text(dc, "Drag and drop a supported image file", 118, 154, width);
    let _ = SetTextColor(dc, COLORREF(0x00a78b62));
    draw_centered_text(
        dc,
        "Press F1 to view shortcuts",
        height - 76,
        height - 46,
        width,
    );

    let _ = SelectObject(dc, old_font);
    let _ = DeleteObject(title_font);
    let _ = DeleteObject(body_font);
}

unsafe fn draw_filename(dc: HDC, width: i32, name: &str) {
    let font = CreateFontW(
        -12,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        0,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Segoe UI"),
    );
    let old_font = SelectObject(dc, font);
    let _ = SetBkMode(dc, TRANSPARENT);
    let _ = SetTextColor(dc, COLORREF(0x00eeeae6));
    let label_width = ((name.chars().count() as i32 * 7) + 18).clamp(70, width);
    let mut encoded: Vec<u16> = name.encode_utf16().collect();
    let mut bounds = RECT {
        left: width - label_width + 6,
        top: 0,
        right: width - 7,
        bottom: 23,
    };
    DrawTextW(
        dc,
        &mut encoded,
        &mut bounds,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
    );
    let _ = SelectObject(dc, old_font);
    let _ = DeleteObject(font);
}

unsafe fn draw_centered_text(dc: HDC, text: &str, top: i32, bottom: i32, width: i32) {
    let mut encoded: Vec<u16> = text.encode_utf16().collect();
    let mut bounds = RECT {
        left: 40,
        top,
        right: width - 40,
        bottom,
    };
    DrawTextW(
        dc,
        &mut encoded,
        &mut bounds,
        DT_CENTER | DT_VCENTER | DT_WORDBREAK,
    );
}

trait OsStringExt {
    fn from_wide(wide: &[u16]) -> OsString;
}

impl OsStringExt for OsString {
    fn from_wide(wide: &[u16]) -> OsString {
        use std::os::windows::ffi::OsStringExt as WindowsOsStringExt;
        WindowsOsStringExt::from_wide(wide)
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_rgba_len, filename_label_width, oriented_dimensions, scale_into_viewport};
    use image::{Rgba, RgbaImage};

    #[test]
    fn render_buffer_size_rejects_invalid_or_excessive_dimensions() {
        assert_eq!(checked_rgba_len(-1, 100), None);
        assert_eq!(checked_rgba_len(100, 0), Some(0));
        assert_eq!(checked_rgba_len(100, 100), Some(40_000));
        assert_eq!(checked_rgba_len(i32::MAX, i32::MAX), None);
    }

    #[test]
    fn fitted_view_is_centered_without_stretching() {
        let image = RgbaImage::from_pixel(2, 1, Rgba([20, 40, 60, 255]));
        let viewport = scale_into_viewport(&image, 4, 4, 1.0);
        assert_eq!(viewport.get_pixel(0, 0)[3], 0);
        assert_eq!(viewport.get_pixel(0, 1)[3], 255);
        assert_eq!(viewport.get_pixel(3, 2)[3], 255);
        assert_eq!(viewport.get_pixel(3, 3)[3], 0);
    }

    #[test]
    fn zoom_crops_inside_a_fixed_viewport() {
        let image = RgbaImage::from_pixel(4, 2, Rgba([20, 40, 60, 255]));
        let viewport = scale_into_viewport(&image, 4, 4, 2.0);
        assert_eq!(viewport.dimensions(), (4, 4));
        assert!(viewport.pixels().all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn quarter_turns_swap_display_dimensions() {
        assert_eq!(oriented_dimensions(1920, 1080, 0), (1920, 1080));
        assert_eq!(oriented_dimensions(1920, 1080, 1), (1080, 1920));
        assert_eq!(oriented_dimensions(1920, 1080, 2), (1920, 1080));
        assert_eq!(oriented_dimensions(1920, 1080, 3), (1080, 1920));
    }

    #[test]
    fn filename_overlay_supports_tiny_image_windows() {
        assert_eq!(filename_label_width("SoupChicken.png", 19), 19);
        assert_eq!(filename_label_width("x.png", 200), 70);
    }
}
