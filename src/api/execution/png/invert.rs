use crate::api::{ApiError, ApiResult};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::Path;

pub(in crate::api::execution) fn write_inverted_png(
    input_path: &Path,
    output_path: &Path,
) -> ApiResult<()> {
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
