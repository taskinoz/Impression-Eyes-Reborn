use std::{fs::File, io::BufReader, path::Path, sync::Arc};

use image::{
    codecs::{gif::GifDecoder, webp::WebPDecoder},
    io::Reader as ImageReader,
    AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageResult, RgbaImage,
};

const MAX_IMAGE_DIMENSION: u32 = 32_768;
const MAX_DECODER_ALLOCATION: u64 = 256 * 1024 * 1024;
const MAX_ANIMATION_FRAMES: usize = 512;
const DEFAULT_FRAME_DELAY_MS: u32 = 100;
const MIN_FRAME_DELAY_MS: u32 = 10;

pub struct DecodedFrame {
    pub image: Arc<RgbaImage>,
    pub delay_ms: u32,
}

pub struct DecodedImage {
    pub frames: Vec<DecodedFrame>,
}

impl DecodedImage {
    pub fn first(&self) -> &DecodedFrame {
        &self.frames[0]
    }
}

pub fn load(path: &Path) -> ImageResult<DynamicImage> {
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    reader.limits(decoder_limits());
    reader.decode()
}

pub fn load_animated(path: &Path) -> ImageResult<DecodedImage> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    match reader.format() {
        Some(ImageFormat::Gif) => {
            let mut decoder = GifDecoder::new(BufReader::new(File::open(path)?))?;
            decoder.set_limits(decoder_limits())?;
            collect_frames(decoder.into_frames())
        }
        Some(ImageFormat::WebP) => {
            let mut decoder = WebPDecoder::new(BufReader::new(File::open(path)?))?;
            decoder.set_limits(decoder_limits())?;
            if decoder.has_animation() {
                collect_frames(decoder.into_frames())
            } else {
                load_static(path)
            }
        }
        _ => load_static(path),
    }
}

fn load_static(path: &Path) -> ImageResult<DecodedImage> {
    let image = load(path)?.to_rgba8();
    Ok(DecodedImage {
        frames: vec![DecodedFrame {
            image: Arc::new(image),
            delay_ms: DEFAULT_FRAME_DELAY_MS,
        }],
    })
}

fn collect_frames(frames: image::Frames<'_>) -> ImageResult<DecodedImage> {
    let mut decoded = Vec::new();
    let mut total_bytes = 0_u64;
    for frame in frames.take(MAX_ANIMATION_FRAMES) {
        let frame = frame?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay_ms = if denominator == 0 {
            DEFAULT_FRAME_DELAY_MS
        } else {
            numerator
                .checked_div(denominator)
                .unwrap_or(DEFAULT_FRAME_DELAY_MS)
                .max(MIN_FRAME_DELAY_MS)
        };
        let image = frame.into_buffer();
        let frame_bytes = u64::from(image.width())
            .saturating_mul(u64::from(image.height()))
            .saturating_mul(4);
        total_bytes = total_bytes.saturating_add(frame_bytes);
        if total_bytes > MAX_DECODER_ALLOCATION {
            break;
        }
        decoded.push(DecodedFrame {
            image: Arc::new(image),
            delay_ms,
        });
    }
    if decoded.is_empty() {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::InsufficientMemory),
        ));
    }
    Ok(DecodedImage { frames: decoded })
}

fn decoder_limits() -> image::io::Limits {
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODER_ALLOCATION);
    limits
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{
        codecs::{gif::GifEncoder, webp::WebPEncoder},
        ColorType, Delay, Frame, ImageEncoder,
    };
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn animated_gif_preserves_frames_and_delays() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ime-reborn-{unique}.gif"));
        let file = File::create(&path).expect("create GIF fixture");
        let mut encoder = GifEncoder::new(file);
        let first = Frame::from_parts(
            RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])),
            0,
            0,
            Delay::from_numer_denom_ms(40, 1),
        );
        let second = Frame::from_parts(
            RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255])),
            0,
            0,
            Delay::from_numer_denom_ms(80, 1),
        );
        encoder
            .encode_frames([first, second])
            .expect("encode GIF fixture");
        drop(encoder);

        let decoded = load_animated(&path).expect("decode animated GIF");
        fs::remove_file(&path).expect("remove GIF fixture");
        assert_eq!(decoded.frames.len(), 2);
        assert_eq!(decoded.frames[0].delay_ms, 40);
        assert_eq!(decoded.frames[1].delay_ms, 80);
    }

    #[test]
    fn static_webp_uses_the_still_image_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ime-reborn-{unique}.webp"));
        let pixels = RgbaImage::from_pixel(3, 2, image::Rgba([12, 34, 56, 255]));
        let file = File::create(&path).expect("create WebP fixture");
        WebPEncoder::new_lossless(file)
            .write_image(pixels.as_raw(), 3, 2, ColorType::Rgba8)
            .expect("encode static WebP");

        let decoded = load_animated(&path).expect("decode static WebP");
        fs::remove_file(&path).expect("remove WebP fixture");
        assert_eq!(decoded.frames.len(), 1);
        assert_eq!(decoded.first().image.dimensions(), (3, 2));
        assert_eq!(decoded.first().image.get_pixel(0, 0).0, [12, 34, 56, 255]);
    }

    #[test]
    fn tiny_png_regression_fixture_decodes() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("SoupChicken.png");
        let decoded = load_animated(&path).expect("decode tiny PNG regression fixture");
        assert_eq!(decoded.frames.len(), 1);
        assert_eq!(decoded.first().image.dimensions(), (19, 24));
    }
}
