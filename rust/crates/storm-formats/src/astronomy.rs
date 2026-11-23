//! Original HIC catalog used by the weather library.
use crate::{FormatError, word};

pub const MAX_STARS: usize = 262_144;
pub const MAX_CATALOG_BYTES: usize = 4 + MAX_STARS * 14;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    /// Right ascension and declination in radians.
    pub angles: [f32; 2],
    pub magnitude: f32,
    pub spectrum: [u8; 2],
}

pub fn read_catalog(bytes: &[u8]) -> Result<Vec<Star>, FormatError> {
    let Some((header, entries)) = bytes.split_at_checked(4) else {
        return Err(FormatError("truncated HIC count"));
    };
    let count = word(header, 0) as usize;
    if count > MAX_STARS || entries.len() != count * 14 {
        return Err(FormatError("invalid HIC catalog record count or length"));
    }
    entries
        .chunks_exact(14)
        .map(|record| {
            let [ra, dec, magnitude] = [0, 1, 2].map(|field| f32::from_bits(word(record, field)));
            if ![ra, dec, magnitude].into_iter().all(f32::is_finite) {
                return Err(FormatError("non-finite HIC catalog record"));
            }
            Ok(Star {
                angles: [ra, dec],
                magnitude,
                spectrum: [record[12], record[13]],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hic_catalog_reads_packed_records_and_rejects_invalid_counts_and_angles() {
        let mut bytes = 1u32.to_le_bytes().to_vec();
        for component in [0.25f32, -0.5, 7.0] {
            bytes.extend(component.to_le_bytes());
        }
        bytes.extend(*b"G2");
        assert_eq!(
            read_catalog(&bytes).unwrap(),
            [Star {
                angles: [0.25, -0.5],
                magnitude: 7.0,
                spectrum: *b"G2"
            }]
        );
        assert!(read_catalog(&bytes[..bytes.len() - 1]).is_err());
        assert!(read_catalog(&u32::MAX.to_le_bytes()).is_err());
        bytes[4..8].copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert!(read_catalog(&bytes).is_err());
    }
}
