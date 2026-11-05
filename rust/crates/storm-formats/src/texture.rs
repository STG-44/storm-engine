//! Portable CPU decoding of 2D TX mip chains into straight-alpha RGBA8.
use crate::{FormatError, TxHeader};

pub struct Mip {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Texture {
    pub header: TxHeader,
    pub mips: Vec<Mip>,
}

const MAX_RGBA_BYTES: u64 = 256 * 1024 * 1024;

fn rgb565(value: u16) -> [u8; 4] {
    let r = (value >> 11) as u8;
    let g = ((value >> 5) & 63) as u8;
    let b = (value & 31) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
        255,
    ]
}

#[allow(clippy::needless_range_loop)] // Interpolation combines the same channel across four colors.
fn bc_colors(block: &[u8], transparent: bool) -> [[u8; 4]; 16] {
    let a = u16::from_le_bytes([block[0], block[1]]);
    let b = u16::from_le_bytes([block[2], block[3]]);
    let mut colors = [rgb565(a), rgb565(b), [0; 4], [0; 4]];
    if !transparent || a > b {
        for channel in 0..3 {
            colors[2][channel] =
                ((2 * u16::from(colors[0][channel]) + u16::from(colors[1][channel])) / 3) as u8;
            colors[3][channel] =
                ((u16::from(colors[0][channel]) + 2 * u16::from(colors[1][channel])) / 3) as u8;
        }
        colors[2][3] = 255;
        colors[3][3] = 255;
    } else {
        for channel in 0..3 {
            colors[2][channel] =
                ((u16::from(colors[0][channel]) + u16::from(colors[1][channel])) / 2) as u8;
        }
        colors[2][3] = 255;
    }
    let selectors = u32::from_le_bytes(block[4..8].try_into().unwrap());
    std::array::from_fn(|i| colors[((selectors >> (i * 2)) & 3) as usize])
}

fn bc_block(block: &[u8], format: u32) -> [[u8; 4]; 16] {
    if format == 0x3154_5844 {
        return bc_colors(block, true);
    }
    let mut pixels = bc_colors(&block[8..], false);
    if matches!(format, 0x3254_5844 | 0x3354_5844) {
        let alpha = u64::from_le_bytes(block[..8].try_into().unwrap());
        for (i, pixel) in pixels.iter_mut().enumerate() {
            pixel[3] = (((alpha >> (i * 4)) & 15) * 17) as u8;
        }
    } else {
        let mut palette = [block[0], block[1], 0, 0, 0, 0, 0, 255];
        if block[0] > block[1] {
            for i in 1..7 {
                palette[i + 1] = (((7 - i) as u16 * u16::from(block[0])
                    + i as u16 * u16::from(block[1]))
                    / 7) as u8;
            }
        } else {
            for i in 1..5 {
                palette[i + 1] = (((5 - i) as u16 * u16::from(block[0])
                    + i as u16 * u16::from(block[1]))
                    / 5) as u8;
            }
        }
        let mut encoded = [0; 8];
        encoded[..6].copy_from_slice(&block[2..8]);
        let selectors = u64::from_le_bytes(encoded);
        for (i, pixel) in pixels.iter_mut().enumerate() {
            pixel[3] = palette[((selectors >> (i * 3)) & 7) as usize];
        }
    }
    if matches!(format, 0x3254_5844 | 0x3454_5844) {
        for pixel in &mut pixels {
            for channel in 0..3 {
                pixel[channel] = if pixel[3] == 0 {
                    0
                } else {
                    (u32::from(pixel[channel]) * 255 / u32::from(pixel[3])).min(255) as u8
                };
            }
        }
    }
    pixels
}

impl Texture {
    pub fn read(bytes: &[u8]) -> Result<Self, FormatError> {
        let header = TxHeader::read(bytes, bytes.len() as u64)?;
        if header.flags & (TxHeader::CUBEMAP | TxHeader::PALETTE) != 0 {
            return Err(FormatError(
                "paletted/cubemap TX decoding is not supported yet",
            ));
        }
        if header.flags != 0 {
            return Err(FormatError("unknown TX storage flags"));
        }
        let block_size = match header.format {
            0x3154_5844 => Some(8_usize),
            0x3254_5844..=0x3554_5844 if header.format & 0x00ff_ffff == 0x0054_5844 => Some(16),
            21..=23 | 25 | 26 | 50 => None,
            _ => return Err(FormatError("unsupported TX pixel format")),
        };
        let mut mips = Vec::new();
        let mut offset = 24_usize;
        let mut budget = 0_u64;
        let mut stored_size = header.mip0_bytes as usize;
        for level in 0..header.mips {
            let width = (header.width >> level).max(1);
            let height = (header.height >> level).max(1);
            let rgba_size = u64::from(width) * u64::from(height) * 4;
            budget = budget
                .checked_add(rgba_size)
                .ok_or(FormatError("TX decoded size overflow"))?;
            if budget > MAX_RGBA_BYTES {
                return Err(FormatError("TX decoded mip chain exceeds 256 MiB limit"));
            }
            let size =
                usize::try_from(rgba_size).map_err(|_| FormatError("TX image is too large"))?;
            let encoded_size = if let Some(block) = block_size {
                width.div_ceil(4) as usize * height.div_ceil(4) as usize * block
            } else {
                width as usize
                    * height as usize
                    * match header.format {
                        21 | 22 => 4,
                        50 => 1,
                        _ => 2,
                    }
            };
            // Storm stores each following mip in one quarter of the previous size.
            // Reject incomplete compressed blocks instead of reading across mip boundaries.
            if stored_size < encoded_size {
                return Err(FormatError(
                    "TX mip byte count is too small for its dimensions",
                ));
            }
            let end = offset
                .checked_add(stored_size)
                .ok_or(FormatError("TX mip offset overflow"))?;
            let encoded = bytes
                .get(offset..end)
                .ok_or(FormatError("truncated TX mip pixels"))?;
            let mut rgba = vec![0; size];
            if let Some(block) = block_size {
                let across = width.div_ceil(4) as usize;
                for (index, data) in encoded[..encoded_size].chunks_exact(block).enumerate() {
                    let pixels = bc_block(data, header.format);
                    for (p, pixel) in pixels.iter().enumerate() {
                        let x = (index % across) * 4 + p % 4;
                        let y = (index / across) * 4 + p / 4;
                        if x < width as usize && y < height as usize {
                            let target = (y * width as usize + x) * 4;
                            rgba[target..target + 4].copy_from_slice(pixel);
                        }
                    }
                }
            } else {
                let stride = encoded_size / (width as usize * height as usize);
                for (input, pixel) in encoded[..encoded_size]
                    .chunks_exact(stride)
                    .zip(rgba.chunks_exact_mut(4))
                {
                    let color = match header.format {
                        21 => [input[2], input[1], input[0], input[3]],
                        22 => [input[2], input[1], input[0], 255],
                        23 => rgb565(u16::from_le_bytes([input[0], input[1]])),
                        25 => {
                            let v = u16::from_le_bytes([input[0], input[1]]);
                            let expand = |x: u16| ((x << 3) | (x >> 2)) as u8;
                            [
                                expand((v >> 10) & 31),
                                expand((v >> 5) & 31),
                                expand(v & 31),
                                if v & 0x8000 == 0 { 0 } else { 255 },
                            ]
                        }
                        26 => {
                            let v = u16::from_le_bytes([input[0], input[1]]);
                            [
                                ((v >> 8) & 15) as u8 * 17,
                                ((v >> 4) & 15) as u8 * 17,
                                (v & 15) as u8 * 17,
                                (v >> 12) as u8 * 17,
                            ]
                        }
                        50 => [input[0], input[0], input[0], 255],
                        _ => unreachable!(),
                    };
                    pixel.copy_from_slice(&color);
                }
            }
            mips.push(Mip {
                width,
                height,
                rgba,
            });
            offset = end;
            stored_size /= 4;
        }
        Ok(Self { header, mips })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(w: u32, h: u32, format: u32, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = [0, w, h, 1, format, pixels.len() as u32]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        bytes.extend_from_slice(pixels);
        bytes
    }
    #[test]
    fn bc1_handles_transparency_and_partial_blocks() {
        let pixels = Texture::read(&file(
            3,
            2,
            0x3154_5844,
            &[0, 0, 255, 255, 255, 255, 255, 255],
        ))
        .unwrap();
        assert_eq!(pixels.mips[0].rgba, vec![0; 3 * 2 * 4]);
        let red = Texture::read(&file(4, 4, 0x3154_5844, &[0, 248, 0, 0, 0, 0, 0, 0])).unwrap();
        assert!(
            red.mips[0]
                .rgba
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
    }
    #[test]
    fn bc2_and_bc3_decode_alpha_independently_of_color_order() {
        let mut block = [255; 16];
        block[8..].copy_from_slice(&[0, 0, 255, 255, 0, 0, 0, 0]);
        assert_eq!(
            &Texture::read(&file(4, 4, 0x3354_5844, &block))
                .unwrap()
                .mips[0]
                .rgba[..4],
            &[0, 0, 0, 255]
        );
        block[..8].copy_from_slice(&[0, 255, 7, 0, 0, 0, 0, 0]);
        let image = Texture::read(&file(4, 4, 0x3554_5844, &block)).unwrap();
        assert_eq!(image.mips[0].rgba[3], 255);
        assert_eq!(image.mips[0].rgba[7], 0);
    }
    #[test]
    fn packed_colors_and_truncated_mips_are_checked() {
        let bytes = file(1, 1, 21, &[3, 2, 1, 128]);
        assert_eq!(Texture::read(&bytes).unwrap().mips[0].rgba, [1, 2, 3, 128]);
        assert_eq!(
            Texture::read(&file(1, 1, 23, &[0, 248])).unwrap().mips[0].rgba,
            [255, 0, 0, 255]
        );
        assert_eq!(
            Texture::read(&file(1, 1, 25, &[0, 124])).unwrap().mips[0].rgba,
            [255, 0, 0, 0]
        );
        assert_eq!(
            Texture::read(&file(1, 1, 26, &[0, 255])).unwrap().mips[0].rgba,
            [255, 0, 0, 255]
        );
        assert!(Texture::read(&bytes[..27]).is_err());
        let mut huge = bytes;
        huge[4..8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(Texture::read(&huge).is_err());
    }

    #[test]
    fn complete_mip_chain_and_its_truncation_are_checked() {
        let red = [0, 248, 0, 0, 0, 0, 0, 0];
        let mut bytes = file(8, 8, 0x3154_5844, &red.repeat(4));
        bytes[12..16].copy_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&red);
        let texture = Texture::read(&bytes).unwrap();
        assert_eq!(texture.mips.len(), 2);
        assert_eq!((texture.mips[1].width, texture.mips[1].height), (4, 4));
        assert!(
            texture.mips[1]
                .rgba
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        assert!(Texture::read(&bytes[..bytes.len() - 1]).is_err());
        assert_eq!(
            Texture::read(&file(1, 1, 22, &[3, 2, 1, 0])).unwrap().mips[0].rgba,
            [1, 2, 3, 255]
        );
    }
}
