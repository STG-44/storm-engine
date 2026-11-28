//! Retained FillIndex and farGrid selection with projected distance as a port.
use crate::sail_drive::Kind;
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub rows: u32,
    pub columns: u32,
}
impl Grid {
    pub const NEAR: Self = Self {
        rows: 17,
        columns: 13,
    };
    pub const ROLLED: Self = Self {
        rows: 5,
        columns: 13,
    };
    /// GetGrid's selection after world transformation and perspective scaling.
    pub fn select(distance: f32, rolled: bool, free: bool) -> Result<Self> {
        ensure!(
            distance.is_finite() && distance >= 0.0,
            "invalid projected sail distance"
        );
        if rolled {
            return Ok(Self::ROLLED);
        }
        if free {
            return Ok(Self {
                rows: 5,
                columns: 4,
            });
        }
        let mut result = Self::NEAR;
        for (rows, columns, limit) in [
            (17, 13, 10.0),
            (13, 13, 20.0),
            (13, 10, 30.0),
            (13, 7, 50.0),
            (9, 7, 200.0),
            (5, 7, 250.0),
            (5, 4, 600.0),
        ] {
            result = Self { rows, columns };
            if distance < limit {
                break;
            }
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    /// Full native allocation, including the untouched middle of an LOD buffer.
    pub indices: Vec<u32>,
    pub normal_count: u32,
    pub hole_start: u32,
    pub hole_count: u32,
}
impl Topology {
    pub fn validate(&self, vertices: usize) -> Result<()> {
        let (n, h, c) = (
            self.normal_count as usize,
            self.hole_start as usize,
            self.hole_count as usize,
        );
        ensure!(
            ((vertices == 153 && self.indices.len() == 768)
                || (vertices == 221 && self.indices.len() == 1152))
                && n <= h
                && h <= self.indices.len()
                && c == self.indices.len() - h
                && n % 3 == 0
                && c % 3 == 0,
            "invalid native sail index ranges"
        );
        ensure!(
            self.indices[..n]
                .iter()
                .chain(&self.indices[h..])
                .all(|v| (*v as usize) < vertices),
            "native sail index outside grid"
        );
        Ok(())
    }
    pub fn packed(&self) -> Result<Vec<u32>> {
        self.validate(if self.indices.len() == 768 { 153 } else { 221 })?;
        Ok(self.indices[..self.normal_count as usize]
            .iter()
            .chain(&self.indices[self.hole_start as usize..])
            .copied()
            .collect())
    }
}
/// Per-mesh FillIndex (sIndx=0), preserving head/tail order and original masks.
pub fn fill(kind: Kind, rolled: bool, grid: Grid, mask: u16) -> Result<Topology> {
    let triangle = kind == Kind::Triangle;
    ensure!(
        matches!(grid.rows, 17 | 13 | 9 | 5 | 3) && matches!(grid.columns, 13 | 10 | 7 | 4),
        "invalid native index LOD"
    );
    ensure!(
        triangle || grid.rows != 3 || grid.columns == 4,
        "quad row-3 topology requires four columns"
    );
    ensure!(
        !rolled || grid == Grid::ROLLED,
        "rolled topology requires native rolled grid"
    );
    let x: &[i32] = match grid.columns {
        13 => &[0, 17, 34, 51, 68],
        10 => &[0, 17, 51, 68],
        7 => &[0, 34, 68],
        4 => &[0, 68],
        _ => unreachable!(),
    };
    let y: &[i32] = match grid.rows {
        17 => &[0, 1, 2, 3, 4],
        13 => &[0, 1, 3, 4],
        9 => &[0, 2, 4],
        5 => &[0, 4],
        3 => &[],
        _ => unreachable!(),
    };
    let capacity = if triangle { 768 } else { 1152 };
    let mut indices = vec![u32::from(u16::MAX); capacity];
    let mut normal = 0;
    let mut hole = capacity;
    let mut put = |section: u32, block: &[i32], ignore_holes: bool| {
        let damaged = !ignore_holes && mask & (1 << section) != 0;
        let start = if damaged {
            hole -= block.len();
            hole
        } else {
            let start = normal;
            normal += block.len();
            start
        };
        for (i, dst) in indices[start..start + block.len()].iter_mut().enumerate() {
            let slot = if triangle && damaged && block.len() == 6 {
                (i + 3) % 6
            } else {
                i
            };
            *dst = block[slot] as u32;
        }
    };
    if triangle && rolled {
        put(
            0,
            &[
                6, 10, 11, 6, 11, 7, 7, 11, 12, 7, 12, 8, 8, 12, 13, 8, 13, 14, 8, 14, 9,
            ],
            true,
        );
        let (mut v1, mut v2, mut d1, mut d2) = (10, 36, 1, 2);
        for ix in 1..4 {
            for _ in 0..4 {
                put(0, &[v1, v2, v2 + d2, v1, v2 + d2, v1 + d1], true);
                v1 += d1;
                v2 += d2;
            }
            d1 += 1;
            d2 += 1;
            v1 += ix * 12 + 10;
            v2 += ix * 12 + 22;
        }
    } else if grid.rows == 3 {
        if triangle {
            put(
                0,
                &[0, 36, 44, 36, 136, 144, 44, 36, 144, 44, 144, 152],
                true,
            );
        } else {
            for ix in 0..grid.columns - 1 {
                let a = ix as i32 * 68;
                put(
                    0,
                    &[
                        a,
                        a + 68,
                        a + 8,
                        a + 68,
                        a + 76,
                        a + 8,
                        a + 8,
                        a + 76,
                        a + 16,
                        a + 76,
                        a + 84,
                        a + 16,
                    ],
                    true,
                );
            }
        }
    } else if triangle {
        let mut section = 0;
        for ix in 0..4 {
            let cx = ix * 4;
            for iy in 0..=ix {
                let cy = (iy - 1) * 4;
                let index = cx * (cx + 1) / 2 + cy;
                for jx in 1..y.len() {
                    let a = y[jx];
                    let b = y[jx - 1];
                    let dx = cx * a + a * (a + 3) / 2;
                    let px = cx * b + b * (b + 3) / 2;
                    for jy in 1..y.len() {
                        let a = y[jy];
                        let b = y[jy - 1];
                        let t = cy + a + y[jx - 1];
                        if t > 0 {
                            put(
                                section,
                                &[
                                    index + a + px,
                                    index + b + px,
                                    index + b + dx,
                                    index + a + px,
                                    index + b + dx,
                                    index + a + dx,
                                ],
                                false,
                            );
                        } else if t == 0 {
                            put(
                                section,
                                &[index + a + px, index + b + dx, index + a + dx],
                                false,
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
                let index = ix * 68 + iy * 4;
                for jx in 0..x.len() - 1 {
                    for jy in 0..y.len() - 1 {
                        let a = index + x[jx] + y[jy];
                        let b = index + x[jx + 1] + y[jy];
                        let c = index + x[jx] + y[jy + 1];
                        let d = index + x[jx + 1] + y[jy + 1];
                        put((ix * 4 + iy) as u32, &[a, b, c, b, d, c], false);
                    }
                }
            }
        }
    }
    let result = Topology {
        indices,
        normal_count: normal as u32,
        hole_start: hole as u32,
        hole_count: (capacity - hole) as u32,
    };
    result.validate(if triangle { 153 } else { 221 })?;
    Ok(result)
}

#[cfg(test)]
mod tests;
