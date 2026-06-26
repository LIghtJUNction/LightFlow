use crate::api::{ApiError, ApiResult};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PngImage {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color_type: png::ColorType,
    pub(super) bit_depth: png::BitDepth,
    pub(super) channels: usize,
    pub(super) data: Vec<u8>,
}

pub(super) fn write_preview_png(
    path: &Path,
    width: u32,
    height: u32,
    seed: u64,
    prompt: &str,
) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut png = encoder
        .write_header()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    let data = preview_pixels(width, height, seed, prompt);

    png.write_image_data(&data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))
}

pub(super) fn read_png_image(path: &Path) -> ApiResult<PngImage> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    let buffer_size = reader.output_buffer_size().ok_or_else(|| {
        ApiError::InvalidRequest(format!("image is too large to decode: {}", path.display()))
    })?;
    let mut data = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;

    data.truncate(info.buffer_size());

    if info.bit_depth != png::BitDepth::Eight {
        return Err(ApiError::InvalidRequest(format!(
            "only 8-bit PNG images are supported: {}",
            path.display()
        )));
    }

    let channels = png_channels(info.color_type).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "indexed PNG images are not supported: {}",
            path.display()
        ))
    })?;

    Ok(PngImage {
        width: info.width,
        height: info.height,
        color_type: info.color_type,
        bit_depth: info.bit_depth,
        channels,
        data,
    })
}

pub(super) fn write_png_image(path: &Path, image: &PngImage) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, image.width, image.height);
    encoder.set_color(image.color_type);
    encoder.set_depth(image.bit_depth);
    let mut png = encoder
        .write_header()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;

    png.write_image_data(&image.data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))
}

pub(super) fn resize_png_image(image: &PngImage, width: u32, height: u32) -> PngImage {
    let mut data = vec![0; width as usize * height as usize * image.channels];

    for y in 0..height {
        let src_y = (u64::from(y) * u64::from(image.height) / u64::from(height)) as u32;
        for x in 0..width {
            let src_x = (u64::from(x) * u64::from(image.width) / u64::from(width)) as u32;
            let src = pixel_offset(src_x, src_y, image.width, image.channels);
            let dst = pixel_offset(x, y, width, image.channels);
            data[dst..dst + image.channels].copy_from_slice(&image.data[src..src + image.channels]);
        }
    }

    PngImage {
        width,
        height,
        color_type: image.color_type,
        bit_depth: image.bit_depth,
        channels: image.channels,
        data,
    }
}

pub(super) fn crop_png_image(
    image: &PngImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> ApiResult<PngImage> {
    if width == 0 || height == 0 || x >= image.width || y >= image.height {
        return Err(ApiError::InvalidRequest(
            "crop rectangle must intersect the source image".to_owned(),
        ));
    }

    let width = width.min(image.width - x);
    let height = height.min(image.height - y);
    let mut data = vec![0; width as usize * height as usize * image.channels];

    for row in 0..height {
        let src = pixel_offset(x, y + row, image.width, image.channels);
        let dst = pixel_offset(0, row, width, image.channels);
        let len = width as usize * image.channels;
        data[dst..dst + len].copy_from_slice(&image.data[src..src + len]);
    }

    Ok(PngImage {
        width,
        height,
        color_type: image.color_type,
        bit_depth: image.bit_depth,
        channels: image.channels,
        data,
    })
}

pub(super) fn preview_edit_image(
    image: &PngImage,
    seed: u64,
    prompt: &str,
    mask: Option<&PngImage>,
) -> PngImage {
    let mut edited = image.clone();
    let color_channels = color_channels(image.color_type);
    let prompt_mix = super::media::stable_seed(prompt);

    for y in 0..image.height {
        for x in 0..image.width {
            let offset = pixel_offset(x, y, image.width, image.channels);
            let mask_strength = mask
                .map(|mask| luminance_at(mask, x, y) as u16)
                .unwrap_or(255);

            if mask_strength == 0 {
                continue;
            }

            let base = seed ^ prompt_mix ^ ((x as u64) << 32) ^ y as u64;
            for channel in 0..color_channels {
                let current = edited.data[offset + channel] as u16;
                let generated = ((base >> (channel * 8)) & 0xff) as u16;
                let blend = (generated * mask_strength + current * (255 - mask_strength)) / 255;
                edited.data[offset + channel] = ((current * 3 + blend) / 4) as u8;
            }
        }
    }

    edited
}

pub(super) fn compose_masks(
    mask_a: &PngImage,
    mask_b: &PngImage,
    mode: &str,
) -> ApiResult<PngImage> {
    let mut data = vec![0; mask_a.width as usize * mask_a.height as usize];

    for y in 0..mask_a.height {
        for x in 0..mask_a.width {
            let a = luminance_at(mask_a, x, y);
            let b = luminance_at(mask_b, x, y);
            let value = match mode {
                "add" => a.saturating_add(b),
                "multiply" | "intersect" => ((u16::from(a) * u16::from(b)) / 255) as u8,
                "min" => a.min(b),
                "subtract" => a.saturating_sub(b),
                "max" | "union" => a.max(b),
                other => {
                    return Err(ApiError::InvalidRequest(format!(
                        "unsupported mask compose mode: {other}"
                    )));
                }
            };

            data[(y as usize * mask_a.width as usize) + x as usize] = value;
        }
    }

    Ok(PngImage {
        width: mask_a.width,
        height: mask_a.height,
        color_type: png::ColorType::Grayscale,
        bit_depth: png::BitDepth::Eight,
        channels: 1,
        data,
    })
}

pub(super) fn luminance_at(image: &PngImage, x: u32, y: u32) -> u8 {
    let offset = pixel_offset(x, y, image.width, image.channels);
    match image.color_type {
        png::ColorType::Rgb | png::ColorType::Rgba => {
            let r = u16::from(image.data[offset]);
            let g = u16::from(image.data[offset + 1]);
            let b = u16::from(image.data[offset + 2]);
            ((r * 77 + g * 150 + b * 29) / 256) as u8
        }
        png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => image.data[offset],
        png::ColorType::Indexed => 0,
    }
}

fn color_channels(color_type: png::ColorType) -> usize {
    match color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 1,
        png::ColorType::Indexed => 0,
    }
}

fn pixel_offset(x: u32, y: u32, width: u32, channels: usize) -> usize {
    (y as usize * width as usize + x as usize) * channels
}

fn png_channels(color_type: png::ColorType) -> Option<usize> {
    match color_type {
        png::ColorType::Rgb => Some(3),
        png::ColorType::Rgba => Some(4),
        png::ColorType::Grayscale => Some(1),
        png::ColorType::GrayscaleAlpha => Some(2),
        png::ColorType::Indexed => None,
    }
}

pub(super) fn write_inverted_png(input_path: &Path, output_path: &Path) -> ApiResult<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::open(input_path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    let buffer_size = reader.output_buffer_size().ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "image is too large to decode: {}",
            input_path.display()
        ))
    })?;
    let mut data = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    data.truncate(info.buffer_size());

    if info.bit_depth != png::BitDepth::Eight {
        return Err(ApiError::InvalidRequest(format!(
            "only 8-bit PNG images can be inverted: {}",
            input_path.display()
        )));
    }

    let color_channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 1,
        png::ColorType::Indexed => {
            return Err(ApiError::InvalidRequest(format!(
                "indexed PNG images are not supported for invert: {}",
                input_path.display()
            )));
        }
    };
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => unreachable!("indexed PNG is rejected above"),
    };

    for pixel in data.chunks_exact_mut(channels) {
        for channel in pixel.iter_mut().take(color_channels) {
            *channel = 255 - *channel;
        }
    }

    let file = fs::File::create(output_path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, info.width, info.height);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    let mut png = encoder
        .write_header()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;

    png.write_image_data(&data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))
}

fn preview_pixels(width: u32, height: u32, seed: u64, prompt: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity((width as usize) * (height as usize) * 3);
    let prompt_mix = super::media::stable_seed(prompt);

    for y in 0..height {
        for x in 0..width {
            let base = seed ^ prompt_mix ^ ((x as u64) << 32) ^ y as u64;
            data.push(((x * 255 / width) as u8) ^ (base as u8));
            data.push(((y * 255 / height) as u8) ^ ((base >> 8) as u8));
            data.push((((x + y) * 127 / (width + height)) as u8) ^ ((base >> 16) as u8));
        }
    }

    data
}
