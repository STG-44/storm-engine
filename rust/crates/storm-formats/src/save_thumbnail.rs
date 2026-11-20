//! The compressed XINTERFACE preview block at the end of a Storm save.
use crate::{texture::Mip, word};
use anyhow::{Result, ensure};
use std::io::{Read, Seek, SeekFrom};

pub const SIDE: u32 = 128;
pub const RGBA_BYTES: usize = (SIDE * SIDE * 4) as usize;
const FILE_HEADER_BYTES: u64 = 40;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_PACKED_BYTES: usize = 256 * 1024;

pub struct SaveThumbnail {
    pub description: String,
    pub image: Mip,
}

/// Reads only the extended save block. The serialized game state can be large
/// and has its own format.
pub fn read(file: &mut (impl Read + Seek)) -> Result<SaveThumbnail> {
    let length = file.seek(SeekFrom::End(0))?;
    file.rewind()?;
    let mut file_header = [0; FILE_HEADER_BYTES as usize];
    file.read_exact(&mut file_header)?;
    let offset = u64::from(word(&file_header, 8));
    let unpacked = word(&file_header, 9) as usize;
    ensure!(
        offset >= FILE_HEADER_BYTES && offset + 4 <= length,
        "save preview offset is outside the file"
    );
    ensure!(
        (8 + RGBA_BYTES..=8 + RGBA_BYTES + MAX_TEXT_BYTES).contains(&unpacked),
        "invalid save preview data size"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut packed = [0; 4];
    file.read_exact(&mut packed)?;
    let packed = u32::from_le_bytes(packed) as usize;
    ensure!(
        (1..=MAX_PACKED_BYTES).contains(&packed) && offset + 4 + packed as u64 <= length,
        "save preview compressed block is outside the file"
    );
    let mut data = Vec::with_capacity(unpacked);
    let mut stream = flate2::read::ZlibDecoder::new(file.take(packed as u64));
    stream
        .by_ref()
        .take(unpacked as u64 + 1)
        .read_to_end(&mut data)?;
    ensure!(
        data.len() == unpacked && stream.total_in() == packed as u64,
        "save preview decompressed size does not match its header"
    );
    let text_size = word(&data, 0) as usize;
    ensure!(
        text_size == unpacked - 8 - RGBA_BYTES && word(&data, 1) as usize == RGBA_BYTES,
        "invalid save preview layout"
    );
    let (text, pixels) = data[8..].split_at_mut(text_size);
    let description = decode_description(text);
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2); // little-endian A8R8G8B8 to RGBA8
    }
    Ok(SaveThumbnail {
        description,
        image: Mip {
            width: SIDE,
            height: SIDE,
            rgba: pixels.to_vec(),
        },
    })
}

fn decode_description(text: &[u8]) -> String {
    let text = text.split(|&byte| byte == 0).next().unwrap_or_default();
    match std::str::from_utf8(text) {
        Ok(text) => text.to_owned(),
        // FixInvalidUtf8 replaces every non-ASCII byte if validation fails.
        Err(_) => text
            .iter()
            .map(|&byte| {
                if byte.is_ascii() {
                    char::from(byte)
                } else {
                    '?'
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn reads_only_the_bounded_zlib_preview_and_restores_channel_order() -> Result<()> {
        let mut file = vec![0u8; 128];
        let text = "Капитан\0".as_bytes();
        let mut preview = Vec::new();
        preview.extend((text.len() as u32).to_le_bytes());
        preview.extend((RGBA_BYTES as u32).to_le_bytes());
        preview.extend(text);
        preview.extend([9, 23, 41, 255].repeat(RGBA_BYTES / 4));
        file[32..36].copy_from_slice(&128u32.to_le_bytes());
        file[36..40].copy_from_slice(&(preview.len() as u32).to_le_bytes());
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(&preview)?;
        let payload = zlib.finish()?;
        file.extend((payload.len() as u32).to_le_bytes());
        file.extend(payload);
        let image = read(&mut Cursor::new(&file))?;
        assert_eq!(image.description, "Капитан");
        assert_eq!(decode_description(b"Ship \xe9\0trailing"), "Ship ?");
        assert_eq!(image.image.width, SIDE);
        assert_eq!(image.image.height, SIDE);
        assert!(
            image
                .image
                .rgba
                .chunks_exact(4)
                .all(|pixel| pixel == [41, 23, 9, 255])
        );
        assert!(read(&mut Cursor::new(&file[..file.len() - 1])).is_err());
        file[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(&mut Cursor::new(&file)).is_err());
        Ok(())
    }
}
