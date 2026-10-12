use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use image::{imageops::FilterType, RgbaImage};

use crate::decoder;

pub const COLUMNS: usize = 7;
const ROWS: usize = 3;
const MAX_ITEMS: usize = COLUMNS * ROWS;
const CACHE_ITEMS: usize = MAX_ITEMS * 4;

pub fn wheel_steps(remainder: &mut i32, delta: i16) -> isize {
    *remainder += i32::from(delta);
    let steps = *remainder / 120;
    *remainder %= 120;
    steps as isize
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified: SystemTime,
}

fn file_stamp(path: &PathBuf) -> Option<FileStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok()?,
    })
}

#[derive(Default)]
pub struct ThumbnailCache {
    entries: VecDeque<ThumbnailItem>,
}

impl ThumbnailCache {
    fn get(&mut self, path: &PathBuf, stamp: Option<FileStamp>) -> Option<ThumbnailItem> {
        let stamp = stamp?;
        let index = self.entries.iter().position(|item| item.path == *path)?;
        let item = self.entries.remove(index)?;
        if item.stamp != Some(stamp) {
            return None;
        }
        self.entries.push_back(item.clone());
        Some(item)
    }

    fn insert(&mut self, item: ThumbnailItem) {
        if !item.loaded || item.stamp.is_none() {
            return;
        }
        self.entries.retain(|old| old.path != item.path);
        self.entries.push_back(item);
        while self.entries.len() > CACHE_ITEMS {
            self.entries.pop_front();
        }
    }
}

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
    stamp: Option<FileStamp>,
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
        Self::at_slot(files, current, files.len().min(MAX_ITEMS) / 2, previous)
    }

    /// The original scrolls files underneath the selected cell, without recentering it.
    pub fn scrolled(files: &[PathBuf], current: usize, previous: &Self) -> Option<Self> {
        Self::at_slot(
            files,
            current,
            previous.hovered.unwrap_or(previous.items.len() / 2),
            Some(previous),
        )
    }

    fn at_slot(
        files: &[PathBuf],
        current: usize,
        slot: usize,
        previous: Option<&Self>,
    ) -> Option<Self> {
        if files.is_empty() {
            return None;
        }
        let current = current.min(files.len() - 1);
        let count = files.len().min(MAX_ITEMS);
        let slot = slot.min(count - 1);
        let items = (0..count)
            .map(|slot_index| {
                let index = (current + files.len() - slot + slot_index) % files.len();
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
                        stamp: None,
                        scaled: Arc::new(Mutex::new(None)),
                    })
            })
            .collect();
        Some(Self {
            items: Arc::new(items),
            hovered: Some(slot),
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
        cache: &mut ThumbnailCache,
        is_current: impl Fn() -> bool,
        mut publish: impl FnMut(Self) -> bool,
    ) -> Option<Self> {
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
            let stamp = file_stamp(&items[index].path);
            if items[index].loaded && stamp.is_some() && items[index].stamp == stamp {
                cache.insert(items[index].clone());
                continue;
            }
            if let Some(cached) = cache.get(&items[index].path, stamp) {
                items[index] = cached;
                if !publish(Self {
                    items: Arc::new(items.clone()),
                    hovered: selected,
                }) {
                    return None;
                }
                continue;
            }
            // A changed file must not reuse either its pixels or its scaled bitmap.
            items[index].loaded = false;
            items[index].scaled = Arc::new(Mutex::new(None));
            items[index].stamp = stamp;
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
                // Do not cache a decode if the file was modified while it was being read.
                if stamp == file_stamp(&items[index].path) {
                    cache.insert(items[index].clone());
                } else {
                    items[index].stamp = None;
                }
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

    #[cfg(test)]
    pub fn hover(&mut self, x: i32, y: i32, width: i32, height: i32) -> bool {
        self.hover_index(
            layout(self.items.len(), width, height)
                .iter()
                .position(|rect| rect.grow(4).contains(x, y)),
        )
    }

    pub fn hover_index(&mut self, index: Option<usize>) -> bool {
        let previous = self.hovered;
        if let Some(index) = index.filter(|index| *index < self.items.len()) {
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
}

struct AnimatedCell {
    item: ThumbnailItem,
    rect: Rect,
    target: Rect,
    steps_left: u8,
    selected: bool,
}

impl AnimatedCell {
    fn tick(&mut self) {
        if self.steps_left == 0 {
            return;
        }
        self.rect = if self.steps_left == 1 {
            self.target
        } else {
            Rect {
                x: (self.rect.x + self.target.x) / 2,
                y: (self.rect.y + self.target.y) / 2,
                width: (self.rect.width + self.target.width) / 2,
                height: (self.rect.height + self.target.height) / 2,
            }
        };
        self.steps_left -= 1;
    }
}

/// Original IME easing: two midpoint steps, then snap, on a 30 ms timer.
#[derive(Default)]
pub struct ThumbnailScene {
    cells: Vec<AnimatedCell>,
}

impl ThumbnailScene {
    pub fn sync(&mut self, overlay: &ThumbnailOverlay, width: i32, height: i32) {
        let mut previous = std::mem::take(&mut self.cells);
        self.cells = overlay
            .items
            .iter()
            .zip(layout(overlay.items.len(), width, height))
            .enumerate()
            .map(|(index, (item, rect))| {
                let selected = overlay.hovered == Some(index);
                let target = if selected {
                    rect.grow(rect.width * 7 / 20)
                } else {
                    rect
                };
                if let Some(old_index) = previous.iter().position(|old| old.item.path == item.path)
                {
                    let mut cell = previous.swap_remove(old_index);
                    cell.item = item.clone();
                    cell.selected = selected;
                    // Progressive loads must not restart a transition already in flight.
                    if cell.target != target {
                        cell.target = target;
                        cell.steps_left = 3;
                    }
                    cell
                } else {
                    AnimatedCell {
                        item: item.clone(),
                        rect,
                        target,
                        steps_left: if rect != target { 3 } else { 0 },
                        selected,
                    }
                }
            })
            .collect();
    }

    pub fn is_animating(&self) -> bool {
        self.cells.iter().any(|cell| cell.steps_left > 0)
    }

    pub fn tick(&mut self) {
        for cell in &mut self.cells {
            cell.tick();
        }
    }

    pub fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        // Match painting order, including the enlarged preview above its neighbours.
        self.cells
            .iter()
            .enumerate()
            .find(|(_, cell)| cell.selected && cell.rect.contains(x, y))
            .or_else(|| {
                self.cells
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, cell)| cell.rect.grow(4).contains(x, y))
            })
            .map(|(index, _)| index)
    }

    pub fn render(&self, pixels: &mut [u8], width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        // Paint the enlarged selection last so neighbouring cells cannot cover it.
        for selected_pass in [false, true] {
            for cell in &self.cells {
                let item = &cell.item;
                let rect = cell.rect;
                let hovered = cell.selected;
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
                draw_thumbnail(pixels, width, height, item, rect);
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

    fn numbered_files(count: usize) -> Vec<PathBuf> {
        (0..count)
            .map(|i| PathBuf::from(format!("{i}.png")))
            .collect()
    }

    #[test]
    fn wheel_accumulates_partial_notches_and_preserves_multi_notch_input() {
        let mut remainder = 0;
        assert_eq!(wheel_steps(&mut remainder, 40), 0);
        assert_eq!(wheel_steps(&mut remainder, 40), 0);
        assert_eq!(wheel_steps(&mut remainder, 40), 1);
        assert_eq!(wheel_steps(&mut remainder, -240), -2);
        assert_eq!(wheel_steps(&mut remainder, 50), 0);
        assert_eq!(wheel_steps(&mut remainder, -50), 0);
        assert_eq!(remainder, 0);
    }

    #[test]
    fn scrolling_keeps_hovered_slot_and_wraps_files_under_it() {
        let files = numbered_files(30);
        let mut old = ThumbnailOverlay::placeholders(&files, 10, None).unwrap();
        old.hover_index(Some(3));
        assert_eq!(old.selected_index(&files), Some(3));
        let next = ThumbnailOverlay::scrolled(&files, 4, &old).unwrap();
        assert_eq!(next.hovered, Some(3));
        assert_eq!(next.selected_index(&files), Some(4));
        assert_eq!(next.items[2].path, old.items[3].path);
        let end = ThumbnailOverlay::scrolled(&files, 29, &next).unwrap();
        let wrapped = ThumbnailOverlay::scrolled(&files, 0, &end).unwrap();
        assert_eq!(wrapped.hovered, Some(3));
        assert_eq!(wrapped.selected_index(&files), Some(0));
    }

    #[test]
    fn scene_scrolls_in_three_steps_and_progressive_refresh_does_not_restart_it() {
        let files = numbered_files(40);
        let old = ThumbnailOverlay::placeholders(&files, 10, None).unwrap();
        let next = ThumbnailOverlay::scrolled(&files, 17, &old).unwrap();
        let mut scene = ThumbnailScene::default();
        scene.sync(&old, 808, 404);
        for _ in 0..3 {
            scene.tick();
        }
        let original = scene
            .cells
            .iter()
            .find(|cell| cell.item.path == files[14])
            .unwrap()
            .rect;
        scene.sync(&next, 808, 404);
        let moving = scene
            .cells
            .iter()
            .find(|cell| cell.item.path == files[14])
            .unwrap();
        assert_eq!(moving.rect, original);
        let target = moving.target;
        assert!(target.y < original.y);
        assert!(scene.is_animating());
        scene.tick();
        let halfway = scene
            .cells
            .iter()
            .find(|cell| cell.item.path == files[14])
            .unwrap()
            .rect;
        assert_eq!(halfway.y, (original.y + target.y) / 2);
        scene.sync(&next, 808, 404);
        assert_eq!(
            scene
                .cells
                .iter()
                .find(|cell| cell.item.path == files[14])
                .unwrap()
                .steps_left,
            2
        );
        scene.tick();
        scene.tick();
        assert!(!scene.is_animating());
        assert_eq!(
            scene
                .cells
                .iter()
                .find(|cell| cell.item.path == files[14])
                .unwrap()
                .rect,
            target
        );
    }

    #[test]
    fn reversing_mid_scroll_starts_from_visible_position_and_hit_tests_it() {
        let files = numbered_files(40);
        let old = ThumbnailOverlay::placeholders(&files, 10, None).unwrap();
        let next = ThumbnailOverlay::scrolled(&files, 11, &old).unwrap();
        let mut scene = ThumbnailScene::default();
        scene.sync(&old, 808, 404);
        for _ in 0..3 {
            scene.tick();
        }
        scene.sync(&next, 808, 404);
        scene.tick();
        let visible = scene.cells[10].rect;
        assert_eq!(
            scene.hit_test(
                visible.x + visible.width / 2,
                visible.y + visible.height / 2
            ),
            Some(10)
        );
        scene.sync(&old, 808, 404);
        assert_eq!(
            scene
                .cells
                .iter()
                .find(|cell| cell.item.path == files[11])
                .unwrap()
                .rect,
            visible
        );
        for _ in 0..3 {
            scene.tick();
        }
        assert!(!scene.is_animating());
    }

    #[test]
    fn cache_is_bounded_and_invalidates_changed_files() {
        let files = numbered_files(CACHE_ITEMS + 1);
        let mut cache = ThumbnailCache::default();
        let stamp = FileStamp {
            length: 12,
            modified: SystemTime::UNIX_EPOCH,
        };
        for file in &files {
            let overlay =
                ThumbnailOverlay::placeholders(std::slice::from_ref(file), 0, None).unwrap();
            let mut item = overlay.items[0].clone();
            item.loaded = true;
            item.stamp = Some(stamp);
            cache.insert(item);
        }
        assert_eq!(cache.entries.len(), CACHE_ITEMS);
        assert!(cache.get(&files[0], Some(stamp)).is_none());
        assert!(cache.get(&files[1], Some(stamp)).is_some());
        assert!(cache
            .get(
                &files[1],
                Some(FileStamp {
                    length: 13,
                    ..stamp
                })
            )
            .is_none());
        assert!(cache.get(&files[1], Some(stamp)).is_none());
        assert!(cache.get(&files[2], None).is_none());
    }

    #[test]
    fn worker_cache_reuses_a_decoded_image_after_the_visible_snapshot_is_gone() {
        let files =
            vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/SoupChicken.png")];
        let mut cache = ThumbnailCache::default();
        let first = ThumbnailOverlay::build_progressive(
            ThumbnailOverlay::placeholders(&files, 0, None).unwrap(),
            &mut cache,
            || true,
            |_| true,
        )
        .unwrap();
        assert!(first.items[0].loaded);
        let decoded = first.items[0].image.clone();
        drop(first);
        let next = ThumbnailOverlay::build_progressive(
            ThumbnailOverlay::placeholders(&files, 0, None).unwrap(),
            &mut cache,
            || true,
            |_| true,
        )
        .unwrap();
        assert!(Arc::ptr_eq(&decoded, &next.items[0].image));
    }

    #[test]
    fn small_folders_scroll_without_duplicates_or_losing_selection() {
        for count in [1, 2, 7, 20] {
            let files = numbered_files(count);
            let mut overlay = ThumbnailOverlay::placeholders(&files, 0, None).unwrap();
            overlay.hover_index(Some(count - 1));
            for current in 0..count {
                overlay = ThumbnailOverlay::scrolled(&files, current, &overlay).unwrap();
                assert_eq!(overlay.hovered, Some(count - 1));
                assert_eq!(overlay.selected_index(&files), Some(current));
                let unique: std::collections::HashSet<_> =
                    overlay.items.iter().map(|item| &item.path).collect();
                assert_eq!(unique.len(), count);
            }
        }
    }

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
            &mut ThumbnailCache::default(),
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
            &mut ThumbnailCache::default(),
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
            stamp: None,
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
                stamp: None,
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
                stamp: None,
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
