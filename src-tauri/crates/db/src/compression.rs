//! Deflate for the two stores whose rows are written once and read rarely (the edition archive and
//! replays). One level for both, with its reason beside it; each store maps the `io::Error` to its
//! own translated error.

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

/// Level 6: measured on generated careers, level 9 saved under 2% more for twice the time, and level 1
/// kept 22% against level 6's 14%.
const LEVEL: u32 = 6;

pub(crate) fn deflate(text: &str) -> std::io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(LEVEL));
    encoder.write_all(text.as_bytes())?;
    encoder.finish()
}

pub(crate) fn inflate(bytes: &[u8]) -> std::io::Result<String> {
    let mut text = String::new();
    ZlibDecoder::new(bytes).read_to_string(&mut text)?;
    Ok(text)
}
