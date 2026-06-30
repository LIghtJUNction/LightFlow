#![allow(dead_code)]

use std::io::BufReader;
use std::path::Path;

pub(crate) fn png_dimensions(path: &Path) -> Result<(u32, u32), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let reader = decoder.read_info()?;
    let info = reader.info();
    Ok((info.width, info.height))
}
