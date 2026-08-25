use std::{
    cmp::Ordering,
    fs,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use windows::{core::PCWSTR, Win32::UI::Shell::StrCmpLogicalW};

pub fn images_in_folder(path: &Path) -> Vec<PathBuf> {
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
    files.sort_by(|left, right| natural_file_name_cmp(left, right));
    if files.is_empty() {
        vec![path.to_path_buf()]
    } else {
        files
    }
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

pub fn is_supported_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "avif"
                | "bmp"
                | "dds"
                | "ff"
                | "gif"
                | "ico"
                | "jpg"
                | "jpeg"
                | "jxl"
                | "pbm"
                | "pgm"
                | "png"
                | "pnm"
                | "ppm"
                | "qoi"
                | "tga"
                | "tif"
                | "tiff"
                | "webp"
        )
    )
}

fn natural_file_name_cmp(left: &Path, right: &Path) -> Ordering {
    let mut left_wide: Vec<u16> = left.file_name().unwrap_or_default().encode_wide().collect();
    let mut right_wide: Vec<u16> = right
        .file_name()
        .unwrap_or_default()
        .encode_wide()
        .collect();
    left_wide.push(0);
    right_wide.push(0);
    let result = unsafe { StrCmpLogicalW(PCWSTR(left_wide.as_ptr()), PCWSTR(right_wide.as_ptr())) };
    result.cmp(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_are_case_insensitive() {
        assert!(is_supported_image(Path::new("photo.PNG")));
        assert!(is_supported_image(Path::new("photo.jpeg")));
        assert!(is_supported_image(Path::new("photo.AVIF")));
        assert!(is_supported_image(Path::new("photo.JXL")));
        assert!(!is_supported_image(Path::new("notes.txt")));
    }

    #[test]
    fn windows_paths_compare_case_insensitively() {
        assert!(same_path(
            Path::new(r"C:\Pictures\Image.PNG"),
            Path::new(r"c:\pictures\image.png")
        ));
    }

    #[test]
    fn natural_order_places_two_before_ten() {
        let mut files = [PathBuf::from("image10.png"), PathBuf::from("image2.png")];
        files.sort_by(|left, right| natural_file_name_cmp(left, right));
        assert_eq!(files[0], PathBuf::from("image2.png"));
    }
}
