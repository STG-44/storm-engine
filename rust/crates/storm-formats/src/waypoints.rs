//! Bounded reader for sailors_way_points.cpp's SIZE/POINT_DATA/LINK_DATA.
use crate::FormatError;
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub struct DeckPoint {
    pub position: [f32; 3],
    pub kind: u8,
}
#[derive(Debug, Clone)]
pub struct DeckGraph {
    pub points: Vec<DeckPoint>,
    pub links: Vec<[usize; 2]>,
    /// Declared trailing records absent in the original INI; never invent edges.
    pub missing_links: usize,
}
impl DeckGraph {
    pub fn read(text: &str) -> Result<Self, FormatError> {
        if text.len() > 1024 * 1024 {
            return Err(FormatError("waypoint INI too large"));
        }
        let mut values = BTreeMap::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_ascii_lowercase();
                continue;
            }
            if let Some((k, v)) = line.split_once('=')
                && values
                    .insert((section.clone(), k.trim().to_ascii_lowercase()), v.trim())
                    .is_some()
            {
                return Err(FormatError("duplicate waypoint field"));
            }
        }
        let get = |s: &str, k: &str| {
            values
                .get(&(s.into(), k.into()))
                .copied()
                .ok_or(FormatError("missing waypoint field"))
        };
        let number = |s: &str| {
            s.parse::<usize>()
                .map_err(|_| FormatError("invalid waypoint count or index"))
        };
        let np = number(get("size", "points")?)?;
        let nl = number(get("size", "links")?)?;
        if !(1..=100).contains(&np) || nl > 10_000 {
            return Err(FormatError("waypoint count exceeds original limits"));
        }
        let mut points = Vec::new();
        for i in 0..np {
            let fields = get("point_data", &format!("point {i}"))?
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(FormatError("invalid waypoint record"));
            }
            let mut position = [0.0; 3];
            for j in 0..3 {
                position[j] = fields[j]
                    .parse::<f32>()
                    .map_err(|_| FormatError("invalid waypoint coordinate"))?;
            }
            let kind = number(fields[3])?;
            if position
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 10_000.0)
                || kind > 10
            {
                return Err(FormatError("invalid waypoint value"));
            }
            points.push(DeckPoint {
                position,
                kind: kind as u8,
            });
        }
        let mut links = Vec::new();
        for i in 0..nl {
            let Some(record) = values.get(&("link_data".into(), format!("link {i}"))) else {
                if (i + 1..nl)
                    .any(|j| values.contains_key(&("link_data".into(), format!("link {j}"))))
                {
                    return Err(FormatError("missing interior waypoint link"));
                }
                break;
            };
            let fields = record.split(',').map(str::trim).collect::<Vec<_>>();
            if fields.len() != 2 {
                return Err(FormatError("invalid waypoint link"));
            }
            let link = [number(fields[0])?, number(fields[1])?];
            if link.iter().any(|&j| j >= np) {
                return Err(FormatError("waypoint link outside point table"));
            }
            links.push(link);
        }
        let missing_links = nl - links.len();
        Ok(Self {
            points,
            links,
            missing_links,
        })
    }
}
