//! Translation of SAILONE::FillIndex at its original full-resolution grid.
use super::*;
pub struct SailIndices {
    pub indices: Vec<u32>,
    pub normal_count: u32,
}
impl SailPatch {
    pub fn original_indices(&self, mask: u16) -> SailIndices {
        let count = if self.is_triangle() {
            16 * 16 * 3
        } else {
            12 * 16 * 6
        };
        let mut indices = vec![0; count];
        let mut normal = 0;
        let mut hole = count;
        let mut put = |section: u32, block: &[i32]| {
            let offset = if mask & (1 << section) != 0 {
                hole -= block.len();
                hole
            } else {
                let start = normal;
                normal += block.len();
                start
            };
            for (i, dst) in indices[offset..offset + block.len()].iter_mut().enumerate() {
                // The original holed triangle branch swaps the two triangles.
                let slot = if self.is_triangle() && mask & (1 << section) != 0 && block.len() == 6 {
                    (i + 3) % 6
                } else {
                    i
                };
                *dst = block[slot] as u32;
            }
        };
        if self.is_triangle() {
            let mut section = 0;
            for ix in 0..4 {
                let cix = ix * 4;
                for iy in 0..=ix {
                    let ciy = (iy - 1) * 4;
                    let idx = cix * (cix + 1) / 2 + ciy;
                    for jx in 1..=4 {
                        let dix = cix * jx + jx * (jx + 3) / 2;
                        let tmpx = cix * (jx - 1) + (jx - 1) * (jx + 2) / 2;
                        for jy in 1..=4 {
                            let tmpy = ciy + jy + jx - 1;
                            if tmpy > 0 {
                                put(
                                    section,
                                    &[
                                        idx + jy + tmpx,
                                        idx + jy - 1 + tmpx,
                                        idx + jy - 1 + dix,
                                        idx + jy + tmpx,
                                        idx + jy - 1 + dix,
                                        idx + jy + dix,
                                    ],
                                );
                            } else if tmpy == 0 {
                                put(
                                    section,
                                    &[idx + jy + tmpx, idx + jy - 1 + dix, idx + jy + dix],
                                );
                            }
                        }
                    }
                    section += 1;
                }
            }
        } else {
            for ix in 0..3 {
                for iy in 0..4 {
                    let idx = ix * 4 * 17 + iy * 4;
                    for jx in 0..4 {
                        for jy in 0..4 {
                            let a = idx + jx * 17 + jy;
                            let b = a + 17;
                            let c = a + 1;
                            let d = b + 1;
                            put((ix * 4 + iy) as u32, &[a, b, c, b, d, c]);
                        }
                    }
                }
            }
        }
        assert_eq!(normal, hole);
        SailIndices {
            indices,
            normal_count: normal as u32,
        }
    }
    /// SAILONE's fifteen/twenty SailPnt coarse trace vertices.
    pub(super) fn trace_cells(&self) -> Vec<(u32, [u32; 3], Option<[u32; 3]>)> {
        let mut out = Vec::new();
        if self.is_triangle() {
            let index = |row: u32, col: u32| {
                let r = row * 4;
                r * (r + 1) / 2 + col * 4
            };
            let mut section = 0;
            for row in 0..4 {
                for col in 0..=row {
                    let a = index(row, col);
                    let b = index(row + 1, col);
                    let c = index(row + 1, col + 1);
                    out.push((
                        section,
                        [a, b, c],
                        (col > 0).then(|| [a, index(row, col - 1), b]),
                    ));
                    section += 1;
                }
            }
        } else {
            for x in 0..3 {
                for y in 0..4 {
                    let a = x * 4 * 17 + y * 4;
                    let b = a + 4 * 17;
                    let c = a + 4;
                    let d = b + 4;
                    out.push((x * 4 + y, [a, b, c], Some([c, b, d])));
                }
            }
        }
        out
    }
}
