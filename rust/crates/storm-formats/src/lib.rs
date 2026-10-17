//! Readers for Storm's original GM geometry and TX textures.
//!
//! Layouts follow `src/libs/geometry/src/rdf.h` and
//! `src/libs/renderer/src/texture.h`. These readers validate metadata and
//! minimum file lengths. The geometry/texture modules decode static meshes
//! and 2D mip pixels; animation and BSP collision remain separate work.

pub mod geometry;
pub mod texture;

use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError(pub &'static str);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Error for FormatError {}

fn word(bytes: &[u8], index: usize) -> u32 {
    u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
}

fn count(bytes: &[u8], index: usize) -> Result<u32, FormatError> {
    let value = word(bytes, index) as i32;
    u32::try_from(value).map_err(|_| FormatError("negative count in asset header"))
}

fn require_length(actual: u64, minimum: u64) -> Result<(), FormatError> {
    if actual < minimum {
        return Err(FormatError(
            "asset file is shorter than its header declares",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct GmHeader {
    pub flags: u32,
    pub name_bytes: u32,
    pub names: u32,
    pub textures: u32,
    pub materials: u32,
    pub lights: u32,
    pub labels: u32,
    pub objects: u32,
    pub triangles: u32,
    pub vertex_buffers: u32,
    pub bounds_size: [f32; 3],
    pub bounds_center: [f32; 3],
    pub radius: f32,
}

impl GmHeader {
    pub const SIZE: usize = 72;

    /// Read the fixed header and check that its tables can fit in `file_len`.
    /// The caller need only supply the first 72 bytes of the file.
    pub fn read(bytes: &[u8], file_len: u64) -> Result<Self, FormatError> {
        if bytes.len() < Self::SIZE {
            return Err(FormatError("truncated GM header"));
        }
        // The C++ multi-character literal '1.05' is stored in little endian.
        if &bytes[..4] != b"50.1" {
            return Err(FormatError("unsupported GM version (expected 1.05)"));
        }
        let floats: [f32; 7] = std::array::from_fn(|i| f32::from_bits(word(bytes, i + 11)));
        if floats.iter().any(|f| !f.is_finite()) || floats[6] < 0.0 {
            return Err(FormatError("invalid GM bounds"));
        }
        let header = Self {
            flags: word(bytes, 1),
            name_bytes: count(bytes, 2)?,
            names: count(bytes, 3)?,
            textures: count(bytes, 4)?,
            materials: count(bytes, 5)?,
            lights: count(bytes, 6)?,
            labels: count(bytes, 7)?,
            objects: count(bytes, 8)?,
            triangles: count(bytes, 9)?,
            vertex_buffers: count(bytes, 10)?,
            bounds_size: [floats[0], floats[1], floats[2]],
            bounds_center: [floats[3], floats[4], floats[5]],
            radius: floats[6],
        };
        require_length(file_len, header.minimum_file_len())?;
        Ok(header)
    }

    /// Tables before vertex data, excluding the variable vertex buffers/BSP.
    /// Signed 32-bit counts and these fixed record sizes cannot overflow u64.
    pub fn minimum_file_len(&self) -> u64 {
        72 + u64::from(self.name_bytes)
            + u64::from(self.names) * 4
            + u64::from(self.textures) * 4
            + u64::from(self.materials) * 56
            + u64::from(self.lights) * 80
            + u64::from(self.labels) * 108
            + u64::from(self.objects) * 104
            + u64::from(self.triangles) * 6
            + u64::from(self.vertex_buffers) * 8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxHeader {
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    pub format: u32,
    pub mip0_bytes: u32,
}

impl TxHeader {
    pub const SIZE: usize = 24;
    pub const PALETTE: u32 = 1;
    pub const CUBEMAP: u32 = 2;

    /// Read metadata and check that at least mip zero and its palette fit.
    /// Use `texture::Texture::read` to decode and validate all 2D mip pixels.
    pub fn read(bytes: &[u8], file_len: u64) -> Result<Self, FormatError> {
        if bytes.len() < Self::SIZE {
            return Err(FormatError("truncated TX header"));
        }
        let header = Self {
            flags: word(bytes, 0),
            width: count(bytes, 1)?,
            height: count(bytes, 2)?,
            mips: count(bytes, 3)?,
            format: word(bytes, 4),
            mip0_bytes: count(bytes, 5)?,
        };
        if header.width == 0 || header.height == 0 || header.mips == 0 {
            return Err(FormatError("zero TX dimensions or mip count"));
        }
        let max_mips = 32 - header.width.max(header.height).leading_zeros();
        if header.mips > max_mips {
            return Err(FormatError("TX mip count exceeds its dimensions"));
        }
        if header.mip0_bytes == 0 {
            return Err(FormatError("empty TX base mip"));
        }
        require_length(file_len, header.minimum_file_len())?;
        Ok(header)
    }

    pub fn minimum_file_len(&self) -> u64 {
        24 + if self.flags & Self::PALETTE != 0 {
            1024
        } else {
            0
        } + u64::from(self.mip0_bytes)
    }

    /// Unknown codes remain available in `format` for future decoders.
    pub fn format_name(&self) -> &'static str {
        match self.format {
            21 => "A8R8G8B8",
            22 => "X8R8G8B8",
            23 => "R5G6B5",
            25 => "A1R5G5B5",
            26 => "A4R4G4B4",
            41 => "P8",
            50 => "L8",
            60 => "V8U8",
            61 => "L6V5U5",
            0x3154_5844 => "DXT1",
            0x3254_5844 => "DXT2",
            0x3354_5844 => "DXT3",
            0x3454_5844 => "DXT4",
            0x3554_5844 => "DXT5",
            _ => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_upstream_cannon_geometry() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/modding-tool/Meshes/cannon_x.gm");
        let bytes = std::fs::read(path).unwrap();
        let header = GmHeader::read(&bytes, bytes.len() as u64).unwrap();
        assert_eq!(header.triangles, 1728);
        assert_eq!(header.objects, 1);
        assert_eq!(header.vertex_buffers, 1);
        assert!(header.minimum_file_len() < bytes.len() as u64);
    }

    #[test]
    fn rejects_truncated_and_impossible_geometry_headers() {
        let mut bytes = [0; 72];
        bytes[..4].copy_from_slice(b"50.1");
        for length in 0..72 {
            assert!(GmHeader::read(&bytes[..length], 72).is_err());
        }
        assert!(GmHeader::read(&bytes, 72).is_ok()); // Empty locator model.
        bytes[36..40].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(GmHeader::read(&bytes, 72).is_err());
        bytes[36..40].copy_from_slice(&(-1_i32).to_le_bytes());
        assert!(GmHeader::read(&bytes, u64::MAX).is_err());
        bytes[36..40].fill(0);
        bytes[68..72].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(GmHeader::read(&bytes, 72).is_err());
    }

    fn tx_header(flags: u32, mips: u32) -> Vec<u8> {
        [flags, 4, 4, mips, 0x3154_5844, 8]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect()
    }

    #[test]
    fn reads_dxt_metadata_and_checks_palette_storage() {
        let bytes = tx_header(0, 3);
        let header = TxHeader::read(&bytes, 48).unwrap();
        assert_eq!((header.width, header.height, header.mips), (4, 4, 3));
        assert_eq!(header.format_name(), "DXT1");
        assert!(TxHeader::read(&bytes, 31).is_err());
        let palette = tx_header(TxHeader::PALETTE, 1);
        assert!(TxHeader::read(&palette, 32).is_err());
        assert!(TxHeader::read(&palette, 1056).is_ok());
    }

    #[test]
    fn rejects_bad_texture_metadata_without_allocating_pixels() {
        let mut bytes = tx_header(0, 1);
        for length in 0..24 {
            assert!(TxHeader::read(&bytes[..length], 32).is_err());
        }
        assert!(TxHeader::read(&tx_header(0, 4), u64::MAX).is_err());
        bytes[4..8].fill(0);
        assert!(TxHeader::read(&bytes, u64::MAX).is_err());
        bytes[4..8].copy_from_slice(&(-1_i32).to_le_bytes());
        assert!(TxHeader::read(&bytes, u64::MAX).is_err());
    }
}
