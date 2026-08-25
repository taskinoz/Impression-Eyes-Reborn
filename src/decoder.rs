use std::{fs, fs::File, io::BufReader, path::Path, sync::Arc};

use image::{
    codecs::{gif::GifDecoder, webp::WebPDecoder},
    io::Reader as ImageReader,
    AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageResult, RgbaImage,
};

const MAX_IMAGE_DIMENSION: u32 = 32_768;
const MAX_DECODER_ALLOCATION: u64 = 256 * 1024 * 1024;
const MAX_SPECIAL_FORMAT_FILE_SIZE: u64 = 256 * 1024 * 1024;
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
    if is_avif(path) {
        return load_avif(path).map(DynamicImage::ImageRgba8);
    }
    if is_jpeg_xl(path) {
        return load_jpeg_xl(path).map(DynamicImage::ImageRgba8);
    }
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    reader.limits(decoder_limits());
    reader.decode()
}

fn is_avif(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("avif"))
}

fn is_jpeg_xl(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jxl"))
}

fn load_avif(path: &Path) -> ImageResult<RgbaImage> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_SPECIAL_FORMAT_FILE_SIZE {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::InsufficientMemory),
        ));
    }
    let bytes = fs::read(path)?;
    let decoded = libavif::decode_rgb(&bytes).map_err(|error| {
        image::ImageError::Decoding(image::error::DecodingError::new(
            image::ImageFormat::Avif.into(),
            error,
        ))
    })?;
    let width = decoded.width();
    let height = decoded.height();
    if width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none_or(|bytes| bytes > MAX_DECODER_ALLOCATION)
    {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }
    RgbaImage::from_raw(width, height, decoded.to_vec()).ok_or_else(|| {
        image::ImageError::Decoding(image::error::DecodingError::new(
            image::ImageFormat::Avif.into(),
            "AVIF decoder returned an invalid RGBA buffer length".to_string(),
        ))
    })
}

fn load_jpeg_xl(path: &Path) -> ImageResult<RgbaImage> {
    if fs::metadata(path)?.len() > MAX_SPECIAL_FORMAT_FILE_SIZE {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::InsufficientMemory),
        ));
    }

    let mut image = jxl_oxide::JxlImage::builder()
        .open(path)
        .map_err(jpeg_xl_decode_error)?;
    validate_rgba_dimensions(image.width(), image.height())?;
    image.request_color_encoding(jxl_oxide::EnumColourEncoding::srgb(
        jxl_oxide::RenderingIntent::Relative,
    ));
    let render = image.render_frame(0).map_err(jpeg_xl_decode_error)?;
    let mut stream = render.stream();
    let width = stream.width();
    let height = stream.height();
    let channels = stream.channels();
    validate_rgba_dimensions(width, height)?;
    if !(1..=4).contains(&channels) {
        return Err(jpeg_xl_decode_error(format!(
            "unsupported JPEG XL channel count: {channels}"
        )));
    }

    let sample_count = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(u64::from(channels)))
        .and_then(|samples| usize::try_from(samples).ok())
        .ok_or_else(|| {
            image::ImageError::Limits(image::error::LimitError::from_kind(
                image::error::LimitErrorKind::InsufficientMemory,
            ))
        })?;
    let mut samples = vec![0_u8; sample_count];
    if stream.write_to_buffer(&mut samples) != sample_count {
        return Err(jpeg_xl_decode_error(
            "JPEG XL decoder returned an incomplete pixel buffer",
        ));
    }

    let pixel_count = usize::try_from(u64::from(width) * u64::from(height)).map_err(|_| {
        image::ImageError::Limits(image::error::LimitError::from_kind(
            image::error::LimitErrorKind::InsufficientMemory,
        ))
    })?;
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    for pixel in samples.chunks_exact(channels as usize) {
        match pixel {
            [gray] => rgba.extend_from_slice(&[*gray, *gray, *gray, 255]),
            [gray, alpha] => rgba.extend_from_slice(&[*gray, *gray, *gray, *alpha]),
            [red, green, blue] => rgba.extend_from_slice(&[*red, *green, *blue, 255]),
            [red, green, blue, alpha] => {
                rgba.extend_from_slice(&[*red, *green, *blue, *alpha]);
            }
            _ => unreachable!("channel count was validated"),
        }
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| jpeg_xl_decode_error("invalid JPEG XL RGBA buffer length"))
}

fn validate_rgba_dimensions(width: u32, height: u32) -> ImageResult<()> {
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none_or(|bytes| bytes > MAX_DECODER_ALLOCATION)
    {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }
    Ok(())
}

fn jpeg_xl_decode_error(
    error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> image::ImageError {
    image::ImageError::Decoding(image::error::DecodingError::new(
        image::error::ImageFormatHint::Name("JPEG XL".to_string()),
        error,
    ))
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

    #[test]
    fn avif_uses_the_bundled_decoder() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("avif-test.avif");
        let decoded = load_animated(&path).expect("decode AVIF fixture");
        assert_eq!(decoded.frames.len(), 1);
        assert_eq!(decoded.first().image.dimensions(), (2, 2));
    }

    #[test]
    fn jpeg_xl_uses_the_pure_rust_decoder() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("jxl-test.jxl");
        let decoded = load_animated(&path).expect("decode JPEG XL fixture");
        assert_eq!(decoded.frames.len(), 1);
        let (width, height) = decoded.first().image.dimensions();
        assert!(width > 0 && height > 0);
    }
}
