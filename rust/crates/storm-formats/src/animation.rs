//! AN float-quaternion skeleton layout: animation_service_imp.cpp::LoadAN.
use crate::{FormatError, word};
#[derive(Debug, Clone)]
pub struct Animation {
    pub frames: usize,
    pub fps: f32,
    pub parents: Vec<Option<usize>>,
    pub bind_positions: Vec<[f32; 3]>,
    pub root_positions: Vec<[f32; 3]>,
    /// Bone-major, then frame-major quaternion XYZW records.
    pub rotations: Vec<[f32; 4]>,
    pub order: Vec<usize>,
}
impl Animation {
    pub fn read(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() < 12 {
            return Err(FormatError("truncated AN header"));
        }
        let frames = word(bytes, 0) as usize;
        let bones = word(bytes, 1) as usize;
        let fps = f32::from_bits(word(bytes, 2));
        if !(1..=100_000).contains(&frames)
            || !(1..=256).contains(&bones)
            || !fps.is_finite()
            || !(0.01..=1000.0).contains(&fps)
        {
            return Err(FormatError("invalid AN dimensions"));
        }
        let size = 12 + bones * 16 + frames * 12 + bones * frames * 16;
        if size > 256 * 1024 * 1024 || bytes.len() != size {
            return Err(FormatError("invalid AN byte length"));
        }
        let mut parents = Vec::new();
        for i in 0..bones {
            let p = word(bytes, 3 + i) as i32;
            if (i == 0 && p != -1) || (i > 0 && (p < 0 || p as usize >= bones || p as usize == i)) {
                return Err(FormatError("invalid AN parent"));
            }
            parents.push(if p < 0 { None } else { Some(p as usize) });
        }
        let mut order = Vec::new();
        while order.len() < bones {
            let before = order.len();
            for (i, p) in parents.iter().enumerate() {
                if !order.contains(&i) && p.is_none_or(|p| order.contains(&p)) {
                    order.push(i);
                }
            }
            if order.len() == before {
                return Err(FormatError("cyclic AN hierarchy"));
            }
        }
        let mut offset = 12 + bones * 4;
        let mut vectors = |n: usize| -> Result<Vec<[f32; 3]>, FormatError> {
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                let v = std::array::from_fn(|j| f32::from_bits(word(&bytes[offset..], j)));
                offset += 12;
                if v.iter().any(|x| !x.is_finite()) {
                    return Err(FormatError("non-finite AN position"));
                }
                out.push(v);
            }
            Ok(out)
        };
        let bind_positions = vectors(bones)?;
        let root_positions = vectors(frames)?;
        let mut rotations = Vec::with_capacity(bones * frames);
        for r in bytes[offset..].chunks_exact(16) {
            let q: [f32; 4] = std::array::from_fn(|j| f32::from_bits(word(r, j)));
            let length = q.iter().map(|v| v * v).sum::<f32>();
            if q.iter().any(|v| !v.is_finite()) || !(0.5..=2.0).contains(&length) {
                return Err(FormatError("invalid AN quaternion"));
            }
            rotations.push(q);
        }
        Ok(Self {
            frames,
            fps,
            parents,
            bind_positions,
            root_positions,
            rotations,
            order,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Clip {
    pub first: usize,
    pub last: usize,
}
impl Clip {
    pub fn from_ini(text: &str, name: &str, frames: usize) -> Result<Self, FormatError> {
        let mut active = false;
        let (mut first, mut last) = (None, None);
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') && line.ends_with(']') {
                active = line[1..line.len() - 1].eq_ignore_ascii_case(name);
                continue;
            }
            if active && let Some((k, v)) = line.split_once('=') {
                let value = || {
                    v.trim()
                        .parse::<usize>()
                        .map_err(|_| FormatError("invalid animation clip frame"))
                };
                match k.trim() {
                    "start_time" => first = Some(value()?),
                    "end_time" => last = Some(value()?),
                    _ => {}
                }
            }
        }
        let result = Self {
            first: first.ok_or(FormatError("missing animation clip start"))?,
            last: last.ok_or(FormatError("missing animation clip end"))?,
        };
        if result.first > result.last || result.last >= frames {
            return Err(FormatError("animation clip outside AN frames"));
        }
        Ok(result)
    }
}
