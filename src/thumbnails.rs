use std::{path::PathBuf, sync::Arc};

use image::{imageops::FilterType, RgbaImage};

use crate::decoder;

const COLUMNS: usize = 7;
const ROWS: usize = 3;
const MAX_ITEMS: usize = COLUMNS * ROWS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Rect {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    fn grow(self, pixels: i32) -> Self {
        Self {
            x: self.x - pixels,
            y: self.y - pixels,
            width: self.width + pixels * 2,
            height: self.height + pixels * 2,
        }
    }
}

pub struct ThumbnailItem {
    path: PathBuf,
    image: RgbaImage,
}

#[derive(Clone)]
pub struct ThumbnailOverlay {
    items: Arc<Vec<ThumbnailItem>>,
    hovered: Option<usize>,
}

impl ThumbnailOverlay {
    pub fn build(files: &[PathBuf], current: usize) -> Option<Self> {
        if files.is_empty() {
            return None;
        }
        let start = centered_window_start(files.len(), current, MAX_ITEMS);
        let end = (start + MAX_ITEMS).min(files.len());
        let items: Vec<_> = files[start..end]
            .iter()
            .filter_map(|path| {
                decoder::load(path).ok().map(|image| ThumbnailItem {
                    path: path.clone(),
                    image: image.thumbnail(160, 160).to_rgba8(),
                })
            })
            .collect();
        if items.is_empty() {
            None
        } else {
            Some(Self {
                items: Arc::new(items),
                hovered: None,
            })
        }
    }

    pub fn hover(&mut self, x: i32, y: i32, width: i32, height: i32) -> bool {
        let previous = self.hovered;
        self.hovered = layout(self.items.len(), width, height)
            .iter()
            .position(|rect| rect.grow(5).contains(x, y));
        self.hovered != previous
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.hovered.map(|index| self.items[index].path.clone())
    }

    pub fn contains_path(&self, path: &PathBuf) -> bool {
        self.items.iter().any(|item| &item.path == path)
    }

    pub fn render(&self, pixels: &mut [u8], width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[0] = (pixel[0] as u16 * 42 / 100) as u8;
            pixel[1] = (pixel[1] as u16 * 42 / 100) as u8;
            pixel[2] = (pixel[2] as u16 * 42 / 100) as u8;
        }
        for (index, (item, rect)) in self
            .items
            .iter()
            .zip(layout(self.items.len(), width, height))
            .enumerate()
        {
            let hovered = self.hovered == Some(index);
            let target = if hovered { rect.grow(8) } else { rect };
            draw_thumbnail(pixels, width, height, &item.image, target);
        }
    }
}

fn centered_window_start(total: usize, current: usize, maximum: usize) -> usize {
    if total <= maximum {
        return 0;
    }
    current
        .saturating_sub(maximum / 2)
        .min(total.saturating_sub(maximum))
}

fn layout(count: usize, width: i32, height: i32) -> Vec<Rect> {
    if count == 0 || width <= 0 || height <= 0 {
        return Vec::new();
    }
    let columns = count.min(COLUMNS);
    let rows = count.div_ceil(columns).min(ROWS);
    let cell = ((width - 32) / columns as i32)
        .min((height - 32) / rows as i32)
        .clamp(36, 112);
    let thumb = (cell - 12).max(28);
    let grid_width = columns as i32 * cell;
    let grid_height = rows as i32 * cell;
    let origin_x = (width - grid_width) / 2;
    let origin_y = (height - grid_height) / 2;
    (0..count)
        .map(|index| {
            let column = index % columns;
            let row = index / columns;
            Rect {
                x: origin_x + column as i32 * cell + (cell - thumb) / 2,
                y: origin_y + row as i32 * cell + (cell - thumb) / 2,
                width: thumb,
                height: thumb,
            }
        })
        .collect()
}

fn draw_thumbnail(
    destination: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    image: &RgbaImage,
    rect: Rect,
) {
    let scale = (rect.width as f64 / image.width().max(1) as f64)
        .min(rect.height as f64 / image.height().max(1) as f64);
    let width = (image.width() as f64 * scale).round().max(1.0) as u32;
    let height = (image.height() as f64 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(image, width, height, FilterType::Triangle);
    let left = rect.x + (rect.width - width as i32) / 2;
    let top = rect.y + (rect.height - height as i32) / 2;
    for (source_x, source_y, pixel) in resized.enumerate_pixels() {
        let x = left + source_x as i32;
        let y = top + source_y as i32;
        if x < 0 || y < 0 || x >= canvas_width || y >= canvas_height {
            continue;
        }
        let offset = ((y * canvas_width + x) * 4) as usize;
        let alpha = pixel[3] as u16;
        let inverse = 255 - alpha;
        for channel in 0..3 {
            destination[offset + channel] = ((pixel[channel] as u16 * alpha
                + destination[offset + channel] as u16 * inverse)
                / 255) as u8;
        }
        destination[offset + 3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn grid_is_centered_and_limited_to_three_rows() {
        let cells = layout(21, 900, 500);
        assert_eq!(cells.len(), 21);
        assert_eq!(cells[0].y, cells[6].y);
        assert!(cells[7].y > cells[0].y);
        let left_margin = cells[0].x;
        let right_margin = 900 - (cells[6].x + cells[6].width);
        assert_eq!(left_margin, right_margin);
    }

    #[test]
    fn subset_stays_centered_near_current_image() {
        assert_eq!(centered_window_start(100, 0, 21), 0);
        assert_eq!(centered_window_start(100, 50, 21), 40);
        assert_eq!(centered_window_start(100, 99, 21), 79);
    }

    #[test]
    fn rectangle_hit_test_excludes_right_and_bottom_edges() {
        let rect = Rect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        };
        assert!(rect.contains(10, 20));
        assert!(!rect.contains(40, 60));
    }

    #[test]
    fn paths_are_retained_for_selection() {
        let item = ThumbnailItem {
            path: Path::new("selected.png").to_path_buf(),
            image: RgbaImage::new(1, 1),
        };
        let overlay = ThumbnailOverlay {
            items: Arc::new(vec![item]),
            hovered: Some(0),
        };
        assert_eq!(overlay.selected_path(), Some(PathBuf::from("selected.png")));
    }

    #[test]
    fn no_hover_means_no_selection() {
        let overlay = ThumbnailOverlay {
            items: Arc::new(vec![ThumbnailItem {
                path: PathBuf::from("current.png"),
                image: RgbaImage::new(1, 1),
            }]),
            hovered: None,
        };
        assert_eq!(overlay.selected_path(), None);
    }
}
