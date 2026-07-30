#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use image::RgbaImage;
use windows::{
    core::w,
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM},
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
            AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
            DIB_RGB_COLORS, HBITMAP, HDC,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW,
                PostQuitMessage, RegisterClassW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, UpdateLayeredWindow, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA,
                HTCAPTION, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, MSG, SWP_NOMOVE, SWP_NOSIZE,
                SW_SHOW, ULW_ALPHA, WM_CREATE, WM_DESTROY, WM_DROPFILES, WM_KEYDOWN, WM_MOUSEWHEEL,
                WM_NCHITTEST, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

const START_WIDTH: i32 = 256;
const START_HEIGHT: i32 = 160;
const KEY_F7: usize = 0x76;
const KEY_F8: usize = 0x77;
const KEY_ESCAPE: usize = 0x1b;

struct AppState {
    image: Option<RgbaImage>,
    transparent: bool,
    topmost: bool,
    files: Vec<PathBuf>,
    current: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            image: None,
            transparent: false,
            topmost: false,
            files: Vec::new(),
            current: 0,
        }
    }
}

static STATE: Mutex<AppState> = Mutex::new(AppState {
    image: None,
    transparent: false,
    topmost: false,
    files: Vec::new(),
    current: 0,
});

fn main() -> windows::core::Result<()> {
    let initial_path = env::args_os().nth(1).map(PathBuf::from);

    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class_name = w!("ImpressionEyesReborneWindow");
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance,
            lpszClassName: class_name,
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        };
        RegisterClassW(&class);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name,
            w!("Impression Eyes Reborne"),
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
        if let Some(path) = initial_path {
            load_image(hwnd, path);
        } else {
            render(hwnd);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
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
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            cycle_image(hwnd, if delta > 0 { -1 } else { 1 });
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTCAPTION as isize),
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn load_image(hwnd: HWND, path: PathBuf) {
    match image::open(&path) {
        Ok(image) => {
            if let Ok(mut state) = STATE.lock() {
                state.image = Some(image.to_rgba8());
                state.files = images_in_folder(&path);
                state.current = state
                    .files
                    .iter()
                    .position(|item| same_path(item, &path))
                    .unwrap_or(0);
            }
            unsafe { render(hwnd) };
        }
        Err(error) => eprintln!("Could not open {}: {error}", path.display()),
    }
}

fn cycle_image(hwnd: HWND, direction: isize) {
    let path = {
        let Ok(mut state) = STATE.lock() else { return };
        if state.files.len() < 2 {
            return;
        }
        let count = state.files.len() as isize;
        state.current = (state.current as isize + direction).rem_euclid(count) as usize;
        state.files[state.current].clone()
    };

    // Keep the existing folder ordering while replacing only the decoded image.
    match image::open(&path) {
        Ok(image) => {
            if let Ok(mut state) = STATE.lock() {
                state.image = Some(image.to_rgba8());
            }
            unsafe { render(hwnd) };
        }
        Err(error) => eprintln!("Could not open {}: {error}", path.display()),
    }
}

fn images_in_folder(path: &Path) -> Vec<PathBuf> {
    let Some(folder) = path.parent() else {
        return vec![path.to_path_buf()];
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return vec![path.to_path_buf()];
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| candidate.is_file() && is_supported_image(candidate))
        .collect();
    files.sort_by(|left, right| {
        left.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
            .cmp(
                &right
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
            )
    });
    if files.is_empty() {
        vec![path.to_path_buf()]
    } else {
        files
    }
}

fn is_supported_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("bmp" | "gif" | "ico" | "jpg" | "jpeg" | "png" | "tif" | "tiff" | "webp")
    )
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

unsafe fn render(hwnd: HWND) {
    let Ok(state) = STATE.lock() else { return };
    let (width, height, mut pixels) = match &state.image {
        Some(image) => (
            image.width() as i32,
            image.height() as i32,
            image.clone().into_raw(),
        ),
        None => {
            let mut pixels = vec![0u8; (START_WIDTH * START_HEIGHT * 4) as usize];
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            (START_WIDTH, START_HEIGHT, pixels)
        }
    };

    // DIB sections use BGRA. Layered windows also require premultiplied alpha.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        let alpha = if state.transparent { pixel[3] } else { 255 } as u16;
        pixel[0] = (pixel[0] as u16 * alpha / 255) as u8;
        pixel[1] = (pixel[1] as u16 * alpha / 255) as u8;
        pixel[2] = (pixel[2] as u16 * alpha / 255) as u8;
        pixel[3] = alpha as u8;
    }

    let screen = HDC::default();
    let memory_dc = CreateCompatibleDC(screen);
    let mut bitmap_info = BITMAPINFO::default();
    bitmap_info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
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

trait OsStringExt {
    fn from_wide(wide: &[u16]) -> OsString;
}

impl OsStringExt for OsString {
    fn from_wide(wide: &[u16]) -> OsString {
        use std::os::windows::ffi::OsStringExt as WindowsOsStringExt;
        WindowsOsStringExt::from_wide(wide)
    }
}
