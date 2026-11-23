//! Retained CoastFoam subdivision and animation in native X/Z coordinates.
use anyhow::{Result, ensure};
use glam::Vec2;
use std::sync::Arc;

pub const ROWS: usize = 8;
pub const MAX_COLUMNS: usize = 5000 / ROWS;
pub const MAX_DRAW_VERTICES: usize = 1_048_576;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Type1,
    Type2,
}

#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub kind: Kind,
    pub pulses: usize,
    pub alpha: [f32; 2],
    pub speed: [f32; 2],
    pub appear: [f32; 2],
    pub texture_scale: f32,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            kind: Kind::Type2,
            pulses: 2,
            alpha: [148.0, 196.0],
            speed: [0.2, 0.3],
            appear: [0.0, 0.2],
            texture_scale: 0.05,
        }
    }
}

impl Parameters {
    fn validate(self) -> Result<()> {
        ensure!(
            (1..=2).contains(&self.pulses)
                && self.speed.into_iter().all(|x| x.is_finite() && x >= 0.0)
                && self.appear.into_iter().all(|x| x.is_finite() && x >= 0.0)
                && self.alpha.into_iter().all(|x| (0.0..=255.0).contains(&x))
                && self.texture_scale.is_finite(),
            "invalid coast foam parameters"
        );
        Ok(())
    }
}

pub trait Random {
    fn scaled(&mut self, maximum: f32) -> f32;
    fn range(&mut self, minimum: f32, maximum: f32) -> f32;
}

/// Advancing past a profile entry with no drawable grid still consumes its native draws.
pub fn skip_initialization(random: &mut impl Random) {
    for _ in 0..14 {
        random.scaled(1.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: u8,
}

#[derive(Clone, Debug)]
pub struct Layout {
    /// Both endpoints of each authored section, before subdivision.
    pub boundary: Arc<[[f32; 2]]>,
    pub origin: [f32; 2],
    /// One index grid shared by every pulse in this strip.
    pub indices: Arc<[u16]>,
}

#[derive(Clone, Copy, Debug, Default)]
enum Mode {
    #[default]
    Advance,
    Brake,
    Retreat,
}

#[derive(Clone, Copy, Debug, Default)]
struct Pulse {
    position: f32,
    speed: f32,
    alpha: f32,
    alpha_color: f32,
    texture_shift: f32,
    mode: Mode,
}

#[derive(Clone, Copy, Debug)]
struct Column {
    start: Vec2,
    span: Vec2,
    u: f32,
    parts: [Part; ROWS],
}

#[derive(Clone, Copy, Debug)]
struct Part {
    position: f32,
    speed: f32,
}

#[derive(Clone)]
pub struct Strip {
    parameters: Parameters,
    columns: Arc<[Column]>,
    layout: Layout,
    pulses: [Pulse; 2],
}

impl Strip {
    /// Each key holds the two endpoints from key_N in their original order.
    pub fn new(
        keys: &[[[f32; 2]; 2]],
        divisions: usize,
        parameters: Parameters,
    ) -> Result<Option<Self>> {
        parameters.validate()?;
        ensure!(
            (2..=MAX_COLUMNS).contains(&divisions),
            "invalid coast foam subdivisions"
        );
        if keys.len() < 2 {
            return Ok(None);
        }
        let count = (keys.len() - 1)
            .checked_mul(divisions - 1)
            .and_then(|n| n.checked_add(1))
            .filter(|n| *n <= MAX_COLUMNS)
            .ok_or_else(|| anyhow::anyhow!("coast foam strip exceeds native vertex capacity"))?;
        ensure!(
            keys.iter().flatten().flatten().all(|v| v.is_finite()),
            "non-finite coast foam keys"
        );
        let mut columns = Vec::with_capacity(count);
        let mut distance = 0.0;
        for (segment, pair) in keys.windows(2).enumerate() {
            let [a, b] = [pair[0], pair[1]].map(|endpoints| endpoints.map(Vec2::from_array));
            let advance = (a[0] - b[0]).length() / divisions as f32;
            for j in usize::from(segment != 0)..divisions {
                let t = j as f32 / (divisions - 1) as f32;
                let start = a[1] + t * (b[1] - a[1]);
                let end = a[0] + t * (b[0] - a[0]);
                let stagger = (columns.len() as f32 / 14.0 * std::f32::consts::PI).sin() * 0.1;
                columns.push(Column {
                    start,
                    span: end - start,
                    u: distance * parameters.texture_scale,
                    parts: std::array::from_fn(|row| Part {
                        position: stagger + row as f32 / 7.0 * 0.4,
                        speed: 2.0,
                    }),
                });
                distance += advance;
            }
        }
        ensure!(
            columns
                .iter()
                .all(|c| c.start.is_finite() && c.span.is_finite() && c.u.is_finite()),
            "coast foam subdivision overflow"
        );
        let indices = (0..ROWS - 1)
            .flat_map(|row| (0..count - 1).map(move |column| (row * count + column) as u16))
            .flat_map(|upper| {
                let lower = upper + count as u16;
                [upper, lower, upper + 1, lower, lower + 1, upper + 1]
            })
            .collect::<Vec<_>>();
        let layout = Layout {
            boundary: keys.iter().flatten().copied().collect(),
            origin: columns[0].start.to_array(),
            indices: indices.into(),
        };
        Ok(Some(Self {
            parameters,
            columns: columns.into(),
            layout,
            pulses: [Pulse::default(); 2],
        }))
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn vertex_budget(&self) -> usize {
        self.columns.len()
            * ROWS
            * match self.parameters.kind {
                Kind::Type1 => 1,
                Kind::Type2 => self.parameters.pulses,
            }
    }

    pub fn initialize(&mut self, random: &mut impl Random) {
        // Load initializes seven values that RecalculateFoam immediately overwrites.
        for _ in 0..7 {
            random.scaled(1.0);
        }
        let p = self.parameters;
        self.pulses[0].position = -random.range(p.appear[0], p.appear[1]);
        self.pulses[1].position = -100_000.0;
        for pulse in &mut self.pulses {
            pulse.speed = random.range(p.speed[0], p.speed[1]);
        }
        for pulse in &mut self.pulses {
            pulse.alpha_color = random.range(p.alpha[0], p.alpha[1]) / 255.0;
        }
        for pulse in &mut self.pulses {
            pulse.texture_shift = random.scaled(100.0);
        }
    }

    pub fn advance(&mut self, delta_ms: u32, random: &mut impl Random) {
        let seconds = delta_ms as f32 * 0.001;
        match self.parameters.kind {
            Kind::Type1 => {
                for column in Arc::make_mut(&mut self.columns) {
                    column.parts[0].position += column.parts[0].speed * seconds * 0.1;
                    if column.parts[0].position >= 0.6 {
                        for part in &mut column.parts {
                            part.speed *= 1.0 - (seconds * 2.0).clamp(0.0, 1.0);
                        }
                    }
                    for part in &mut column.parts[1..] {
                        part.position += part.speed * seconds * 0.1;
                    }
                }
            }
            Kind::Type2 => self.advance_pulses(seconds, random),
        }
    }

    fn advance_pulses(&mut self, seconds: f32, random: &mut impl Random) {
        let p = self.parameters;
        for index in 0..p.pulses {
            let other = 1 - index;
            let mut pulse = self.pulses[index];
            if pulse.position < 0.0 {
                pulse.position += seconds;
                if pulse.position < 0.0 {
                    self.pulses[index] = pulse;
                    continue;
                }
                pulse.position = 0.0;
            }
            match pulse.mode {
                Mode::Advance => {
                    pulse.alpha = (pulse.alpha + seconds * 0.15).clamp(0.0, 1.0);
                    if pulse.position > 0.6 {
                        pulse.mode = Mode::Brake;
                    }
                }
                Mode::Brake => {
                    pulse.alpha = (pulse.alpha + seconds * 0.15).clamp(0.0, 1.0);
                    pulse.speed *= 1.0 - (seconds * 0.8).clamp(0.0, 1.0);
                    if pulse.speed < 0.01 {
                        if self.pulses[other].position < 0.0 {
                            let next = &mut self.pulses[other];
                            next.texture_shift = random.scaled(100.0);
                            next.alpha = 0.0;
                            next.mode = Mode::Advance;
                            next.speed = random.range(p.speed[0], p.speed[1]);
                            next.position = 0.0;
                            pulse.alpha_color = random.range(p.alpha[0], p.alpha[1]) / 255.0;
                        }
                        pulse.mode = Mode::Retreat;
                    }
                }
                Mode::Retreat => {
                    pulse.speed = (pulse.speed - seconds * 0.035).max(-0.04);
                    pulse.alpha = (pulse.alpha - seconds * 0.2).clamp(0.0, 1.0);
                    if pulse.alpha <= 0.0 {
                        pulse.texture_shift = random.scaled(100.0);
                        pulse.mode = Mode::Advance;
                        pulse.speed = random.range(p.speed[0], p.speed[1]);
                        pulse.position = -random.range(p.appear[0], p.appear[1]);
                        pulse.alpha_color = random.range(p.alpha[0], p.alpha[1]) / 255.0;
                    }
                }
            }
            pulse.position += pulse.speed * seconds;
            self.pulses[index] = pulse;
        }
    }

    /// The caller supplies the current SEA WaveXZ height in native coordinates.
    pub fn layers(
        &self,
        height_offset: f32,
        mut height: impl FnMut(f32, f32) -> Result<f32>,
    ) -> Result<Vec<Vec<Vertex>>> {
        ensure!(height_offset.is_finite(), "non-finite coast foam height");
        match self.parameters.kind {
            Kind::Type1 => Ok(vec![self.vertices(None, height_offset, &mut height)?]),
            Kind::Type2 => self.pulses[..self.parameters.pulses]
                .iter()
                .filter(|pulse| pulse.alpha > 0.0 && pulse.position >= 0.0)
                .map(|pulse| self.vertices(Some(*pulse), height_offset, &mut height))
                .collect(),
        }
    }

    fn vertices(
        &self,
        pulse: Option<Pulse>,
        height_offset: f32,
        height: &mut impl FnMut(f32, f32) -> Result<f32>,
    ) -> Result<Vec<Vertex>> {
        let count = self.columns.len();
        let mut vertices = Vec::with_capacity(count * ROWS);
        for row in 0..ROWS {
            let t = row as f32 / 7.0;
            let profile = (2.5 * (1.0 - t) * t).clamp(0.0, 1.0);
            for (column, part) in self.columns.iter().enumerate() {
                let left = if column <= 4 {
                    column as f32 / 4.0
                } else {
                    1.0
                };
                let remaining = count - 1 - column;
                let (travel, alpha, uv) = if let Some(pulse) = pulse {
                    let border = if remaining <= 4 {
                        remaining as f32 / 4.0
                    } else {
                        left
                    };
                    (
                        pulse.position * row as f32 / 7.0,
                        pulse.alpha_color * (pulse.alpha * profile) * border,
                        [pulse.texture_shift + part.u, 1.0 - t],
                    )
                } else {
                    let travel = part.parts[row].position;
                    let fade = if travel > 0.9 {
                        (1.0 - travel).clamp(0.0, 1.0) / 0.1
                    } else {
                        1.0
                    };
                    let right = if remaining <= 3 {
                        remaining as f32 / 4.0
                    } else {
                        1.0
                    };
                    (travel, profile * fade * left * right, [part.u, t * 2.0])
                };
                let point = part.start + travel * part.span;
                let y = height(point.x, point.y)? + height_offset;
                ensure!(
                    [point.x, y, point.y]
                        .into_iter()
                        .chain(uv)
                        .chain([alpha])
                        .all(f32::is_finite),
                    "coast foam vertex overflow"
                );
                vertices.push(Vertex {
                    position: [point.x, y, point.y],
                    uv,
                    alpha: (alpha * 255.0) as u8,
                });
            }
        }
        Ok(vertices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Midpoint {
        draws: usize,
    }

    impl Random for Midpoint {
        fn scaled(&mut self, maximum: f32) -> f32 {
            self.draws += 1;
            maximum * 0.5
        }

        fn range(&mut self, minimum: f32, maximum: f32) -> f32 {
            self.draws += 1;
            (minimum + maximum) * 0.5
        }
    }

    const KEYS: [[[f32; 2]; 2]; 3] = [
        [[0.0, 12.0], [0.0, 4.0]],
        [[8.0, 12.0], [8.0, 4.0]],
        [[20.0, 12.0], [20.0, 4.0]],
    ];

    #[test]
    fn second_pulse_reuses_the_strip_and_follows_wave_height() -> Result<()> {
        let mut strip = Strip::new(
            &KEYS,
            4,
            Parameters {
                alpha: [255.0; 2],
                speed: [0.25; 2],
                appear: [0.0; 2],
                ..Parameters::default()
            },
        )?
        .unwrap();
        let mut random = Midpoint::default();
        strip.initialize(&mut random);
        assert_eq!(random.draws, 14);
        assert_eq!(strip.layout().origin, [0.0, 4.0]);
        assert_eq!(strip.layout().indices.len(), 7 * 6 * 6);
        assert_eq!(&strip.layout().indices[..6], &[0, 7, 1, 7, 8, 1]);
        assert!(strip.layers(0.2, |_, _| Ok(0.0))?.is_empty());

        strip.advance(1000, &mut random);
        let draw = strip.layers(0.2, |x, z| Ok(x + 0.5 * z))?;
        assert_eq!(draw.len(), 1);
        let center = draw[0][24];
        assert_eq!(center.position[0], 8.0);
        assert!((center.position[2] - (4.0 + 0.25 * 8.0 * 3.0 / 7.0)).abs() < 1e-6);
        assert!((center.position[1] - (8.2 + center.position[2] * 0.5)).abs() < 1e-6);
        assert!((center.uv[0] - 50.3).abs() < 1e-5);
        assert_eq!(center.alpha, 17);
        assert_eq!(draw[0][21].alpha, 0);

        for _ in 0..6 {
            strip.advance(1000, &mut random);
        }
        let draw = strip.layers(0.2, |_, _| Ok(0.0))?;
        assert_eq!(draw.len(), 2);
        assert!(matches!(strip.pulses[0].mode, Mode::Retreat));
        assert!(matches!(strip.pulses[1].mode, Mode::Advance));
        assert_eq!(random.draws, 17);
        assert_ne!(draw[0][24].position, draw[1][24].position);
        Ok(())
    }

    #[test]
    fn type_one_rows_keep_their_stagger_and_damp_together() -> Result<()> {
        let mut strip = Strip::new(
            &KEYS,
            4,
            Parameters {
                kind: Kind::Type1,
                ..Parameters::default()
            },
        )?
        .unwrap();
        let mut random = Midpoint::default();
        strip.initialize(&mut random);
        let before = strip.clone();
        for _ in 0..6 {
            strip.advance(500, &mut random);
        }
        let draw = strip.layers(0.2, |_, z| Ok(1.0 + z))?;
        let original = before.layers(0.2, |_, z| Ok(1.0 + z))?;
        assert_eq!(draw.len(), 1);
        assert_eq!(draw[0][49].uv[1], 2.0);
        assert_eq!(draw[0][49].alpha, 0);
        assert!(draw[0][24].position[2] > original[0][24].position[2]);
        assert!(strip.columns[0].parts.iter().all(|part| part.speed == 0.0));
        assert!(draw[0][24].alpha > 0);
        assert_eq!(random.draws, 14);
        Ok(())
    }
}
