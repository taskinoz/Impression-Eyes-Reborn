use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use image::{imageops::FilterType, RgbaImage};

use crate::decoder;

pub const COLUMNS: usize = 7;
const ROWS: usize = 3;
const MAX_ITEMS: usize = COLUMNS * ROWS;
static DECODE_LOCK: Mutex<()> = Mutex::new(());

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

#[derive(Clone)]
pub struct ThumbnailItem {
    path: PathBuf,
    image: Arc<RgbaImage>,
    original_width: u32,
    original_height: u32,
    loaded: bool,
    scaled: Arc<Mutex<Option<(u32, u32, Arc<RgbaImage>)>>>,
}

pub struct ImagePreview {
    pub image: Arc<RgbaImage>,
    pub original_width: u32,
    pub original_height: u32,
}

#[derive(Clone)]
pub struct ThumbnailOverlay {
    items: Arc<Vec<ThumbnailItem>>,
    hovered: Option<usize>,
}

impl ThumbnailOverlay {
    pub fn selected_index(&self, files: &[PathBuf]) -> Option<usize> {
        self.selected_path()
            .and_then(|path| files.iter().position(|file| *file == path))
    }

    pub fn placeholders(
        files: &[PathBuf],
        current: usize,
        previous: Option<&Self>,
    ) -> Option<Self> {
        if files.is_empty() {
            return None;
        }
        let current = current.min(files.len() - 1);
        let count = files.len().min(MAX_ITEMS);
        let items = (0..count)
            .map(|slot| {
                let index = (current + files.len() - count / 2 + slot) % files.len();
                let path = &files[index];
                previous
                    .and_then(|old| old.items.iter().find(|item| item.path == *path))
                    .cloned()
                    .unwrap_or_else(|| ThumbnailItem {
                        path: path.clone(),
                        image: Arc::new(RgbaImage::new(1, 1)),
                        original_width: 1,
                        original_height: 1,
                        loaded: false,
                        scaled: Arc::new(Mutex::new(None)),
                    })
            })
            .collect();
        Some(Self {
            items: Arc::new(items),
            hovered: Some(count / 2),
        })
    }

    pub fn preserving_hover(mut self, previous: &Self) -> Self {
        let hovered_path = previous
            .hovered
            .and_then(|index| previous.items.get(index))
            .map(|item| &item.path);
        self.hovered = hovered_path.and_then(|path| {
            self.items
                .iter()
                .position(|item| item.path.as_path() == path.as_path())
        });
        self
    }

    pub fn build_progressive(
        initial: Self,
        is_current: impl Fn() -> bool,
        mut publish: impl FnMut(Self) -> bool,
    ) -> Option<Self> {
        // Serialize heavy decodes; superseded jobs exit before opening another file.
        let _guard = DECODE_LOCK.lock().ok()?;
        if !is_current() {
            return None;
        }
        let _com = crate::shell_thumbnail::ComApartment::new();
        let mut items = initial.items.as_ref().clone();
        let selected = initial.hovered;
        if !publish(initial) {
            return None;
        }

        // Match the original's centre-out loading: selection first, then neighbours.
        let mut decode_order: Vec<usize> = (0..items.len()).collect();
        decode_order.sort_by_key(|index| index.abs_diff(selected.unwrap_or(items.len() / 2)));
        for index in decode_order {
            if !is_current() {
                return None;
            }
            if items[index].loaded {
                continue;
            }
            let path = &items[index].path;
            let decoded = crate::shell_thumbnail::load(path).or_else(|| {
                decoder::load(path).ok().map(|image| {
                    let dimensions = (image.width(), image.height());
                    (image.thumbnail(163, 163).to_rgba8(), dimensions)
                })
            });
            if let Some((image, (width, height))) = decoded {
                items[index].original_width = width;
                items[index].original_height = height;
                items[index].image = Arc::new(image);
                items[index].loaded = true;
            }
            // Publish failures too, so cancellation does not depend on successful decoding.
            if !publish(Self {
                items: Arc::new(items.clone()),
                hovered: selected,
            }) {
                return None;
            }
        }
        Some(Self {
            items: Arc::new(items),
            hovered: selected,
        })
    }

    pub fn hover(&mut self, x: i32, y: i32, width: i32, height: i32) -> bool {
        let previous = self.hovered;
        if let Some(index) = layout(self.items.len(), width, height)
            .iter()
            .position(|rect| rect.grow(4).contains(x, y))
        {
            self.hovered = Some(index);
        }
        self.hovered != previous
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.hovered.map(|index| self.items[index].path.clone())
    }

    pub fn preview_for_path(&self, path: &PathBuf) -> Option<ImagePreview> {
        self.items
            .iter()
            .find(|item| &item.path == path)
            .filter(|item| item.loaded)
            .map(|item| ImagePreview {
                image: item.image.clone(),
                original_width: item.original_width,
                original_height: item.original_height,
            })
    }

    pub fn render(&self, pixels: &mut [u8], width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        // Paint the enlarged selection last so neighbouring cells cannot cover it.
        for selected_pass in [false, true] {
            for (index, (item, rect)) in self
                .items
                .iter()
                .zip(layout(self.items.len(), width, height))
                .enumerate()
            {
                let hovered = self.hovered == Some(index);
                if hovered != selected_pass {
                    continue;
                }
                if !item.loaded {
                    for y in rect.y.max(0)..(rect.y + rect.height).min(height) {
                        for x in rect.x.max(0)..(rect.x + rect.width).min(width) {
                            let offset = ((y * width + x) * 4) as usize;
                            pixels[offset..offset + 4].copy_from_slice(if hovered {
                                &[100, 120, 150, 255]
                            } else {
                                &[50, 50, 50, 255]
                            });
                        }
                    }
                    continue;
                }
                let target = if hovered {
                    rect.grow(rect.width * 7 / 20)
                } else {
                    rect
                };
                draw_thumbnail(pixels, width, height, item, target);
            }
        }
    }
}

fn layout(count: usize, width: i32, height: i32) -> Vec<Rect> {
    if count == 0 || width <= 0 || height <= 0 {
        return Vec::new();
    }
    let columns = count.min(COLUMNS);
    let rows = count.div_ceil(columns).min(ROWS);
    let cell = ((width - 80).max(1) / columns as i32)
        .min((height - 80).max(1) / rows as i32)
        .clamp(1, 104);
    let thumb = (cell - 8).max(1);
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
                y: origin_y
                    + row as i32 * cell
                    + (cell - thumb) / 2
                    + (column.min(columns - 1 - column) as i32 * 4),
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
    item: &ThumbnailItem,
    rect: Rect,
) {
    let image = &item.image;
    let scale = (rect.width as f64 / image.width().max(1) as f64)
        .min(rect.height as f64 / image.height().max(1) as f64);
    let width = (image.width() as f64 * scale).round().max(1.0) as u32;
    let height = (image.height() as f64 * scale).round().max(1.0) as u32;
    let resized = {
        let mut cache = item
            .scaled
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((w, h, image)) = cache
            .as_ref()
            .filter(|(w, h, _)| *w == width && *h == height)
        {
            let _ = (w, h);
            image.clone()
        } else {
            let resized = Arc::new(image::imageops::resize(
                image.as_ref(),
                width,
                height,
                FilterType::Triangle,
            ));
            *cache = Some((width, height, resized.clone()));
            resized
        }
    };
    let left = rect.x + (rect.width - width as i32) / 2;
    let top = rect.y + (rect.height - height as i32) / 2;
    for (source_x, source_y, pixel) in resized.enumerate_pixels() {
        let x = left + source_x as i32;
        let y = top + source_y as i32;
        if x < 0 || y < 0 || x >= canvas_width || y >= canvas_height {
            continue;
        }
        let offset = ((y * canvas_width + x) * 4) as usize;
        let alpha = pixel[3] as u32;
        let inverse = 255 - alpha;
        let background_alpha = destination[offset + 3] as u32;
        let combined = alpha * 255 + background_alpha * inverse;
        for channel in 0..3 {
            destination[offset + channel] = if combined == 0 {
                0
            } else {
                ((pixel[channel] as u32 * alpha * 255
                    + destination[offset + channel] as u32 * background_alpha * inverse)
                    / combined) as u8
            };
        }
        destination[offset + 3] = (combined / 255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn scrolling_reuses_loaded_pixels_and_scaled_cache() {
        let files: Vec<_> = (0..30).map(|i| PathBuf::from(format!("{i}.png"))).collect();
        let mut old = ThumbnailOverlay::placeholders(&files, 10, None).unwrap();
        Arc::make_mut(&mut old.items)[10].loaded = true;
        let next = ThumbnailOverlay::placeholders(&files, 11, Some(&old)).unwrap();
        let reused = next
            .items
            .iter()
            .find(|item| item.path == files[10])
            .unwrap();
        assert!(reused.loaded);
        assert!(Arc::ptr_eq(&reused.image, &old.items[10].image));
        assert!(Arc::ptr_eq(&reused.scaled, &old.items[10].scaled));
    }

    #[test]
    fn cancelled_job_does_not_publish_or_decode() {
        let overlay =
            ThumbnailOverlay::placeholders(&[PathBuf::from("missing.png")], 0, None).unwrap();
        let result = ThumbnailOverlay::build_progressive(
            overlay,
            || false,
            |_| panic!("stale job published"),
        );
        assert!(result.is_none());
    }

    #[test]
    fn failed_decodes_still_check_cancellation() {
        let overlay =
            ThumbnailOverlay::placeholders(&[PathBuf::from("missing.png")], 0, None).unwrap();
        let mut publications = 0;
        let result = ThumbnailOverlay::build_progressive(
            overlay,
            || true,
            |_| {
                publications += 1;
                publications < 2
            },
        );
        assert!(result.is_none());
        assert_eq!(publications, 2);
    }

    #[test]
    fn transparent_thumbnail_preserves_canvas_alpha_and_scaling_is_reused() {
        let mut overlay =
            ThumbnailOverlay::placeholders(&[PathBuf::from("test.png")], 0, None).unwrap();
        let item = &mut Arc::make_mut(&mut overlay.items)[0];
        item.image = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([200, 80, 40, 128])));
        let mut pixels = vec![0; 16];
        let rect = Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        draw_thumbnail(&mut pixels, 2, 2, item, rect);
        assert_eq!(&pixels[..4], &[200, 80, 40, 128]);
        let first = item.scaled.lock().unwrap().as_ref().unwrap().2.clone();
        draw_thumbnail(&mut pixels, 2, 2, item, rect);
        assert!(Arc::ptr_eq(
            &first,
            &item.scaled.lock().unwrap().as_ref().unwrap().2
        ));
    }

    #[test]
    fn moving_between_cells_keeps_keyboard_selection() {
        let mut overlay =
            ThumbnailOverlay::placeholders(&[PathBuf::from("test.png")], 0, None).unwrap();
        assert!(!overlay.hover(0, 0, 808, 404));
        assert_eq!(overlay.selected_path(), Some(PathBuf::from("test.png")));
    }

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
    fn wrapped_grid_centers_current_without_duplicates() {
        let files: Vec<_> = (0..100)
            .map(|index| PathBuf::from(format!("{index}.png")))
            .collect();
        for current in [0, 50, 99] {
            let overlay = ThumbnailOverlay::placeholders(&files, current, None).unwrap();
            assert_eq!(overlay.selected_index(&files), Some(current));
            assert_eq!(overlay.items.len(), 21);
            let unique: std::collections::HashSet<_> =
                overlay.items.iter().map(|item| &item.path).collect();
            assert_eq!(unique.len(), 21);
        }
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
            image: Arc::new(RgbaImage::new(1, 1)),
            original_width: 1,
            original_height: 1,
            loaded: true,
            scaled: Arc::new(Mutex::new(None)),
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
                image: Arc::new(RgbaImage::new(1, 1)),
                original_width: 1,
                original_height: 1,
                loaded: true,
                scaled: Arc::new(Mutex::new(None)),
            }]),
            hovered: None,
        };
        assert_eq!(overlay.selected_path(), None);
    }

    #[test]
    fn progressive_refresh_preserves_hovered_path() {
        let old = ThumbnailOverlay {
            items: Arc::new(vec![ThumbnailItem {
                path: PathBuf::from("selected.png"),
                image: Arc::new(RgbaImage::new(1, 1)),
                original_width: 1,
                original_height: 1,
                loaded: false,
                scaled: Arc::new(Mutex::new(None)),
            }]),
            hovered: Some(0),
        };
        let refreshed = old.clone().preserving_hover(&old);
        assert_eq!(
            refreshed.selected_path(),
            Some(PathBuf::from("selected.png"))
        );
    }
}
