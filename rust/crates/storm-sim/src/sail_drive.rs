//! Retained sail wind/damage arithmetic and MSG_SAIL_GET_SPEED selection.
//! Matrices, rolling geometry and the entity message transport are supplied ports.
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Kind {
    Triangle,
    Square,
    Trapezoid,
    Gerbed,
    Rolling,
    Special,
}
#[derive(Clone, Copy, Debug)]
pub struct Coefficients {
    pub triangle: [f32; 3],
    pub trapezoid: [f32; 3],
    pub square: [f32; 3],
}
impl Default for Coefficients {
    fn default() -> Self {
        Self {
            triangle: [0.2, 0.6, 0.8],
            trapezoid: [0.4, 0.5, 0.6],
            square: [0.4, 0.1, 0.6],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Sail {
    pub kind: Kind,
    pub maximum: f32,
    pub holes: i32,
    pub hole_maximum: i32,
    pub hole_dependence: f32,
    /// Native matrix columns: MulToInvNorm takes a dot product with each.
    pub axes: [[f32; 3]; 3],
    pub rolling: bool,
    pub rolling_delta: Option<f32>,
    pub roll_type: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct SailOutput {
    pub wind: [f32; 3],
    pub current: f32,
    pub maximum: f32,
}
impl Sail {
    pub fn evaluate(&self, wind: [f32; 3], coefficients: Coefficients) -> Result<SailOutput> {
        ensure!(
            wind.into_iter()
                .chain(self.axes.into_iter().flatten())
                .chain([self.maximum, self.hole_dependence])
                .chain(self.rolling_delta)
                .chain(coefficients.triangle)
                .chain(coefficients.trapezoid)
                .chain(coefficients.square)
                .all(f32::is_finite),
            "non-finite sail drive input"
        );
        let local = self
            .axes
            .map(|a| wind[0] * a[0] + wind[1] * a[1] + wind[2] * a[2]);
        let mut maximum = self.maximum;
        if self.holes > 0 {
            maximum *=
                1.0 - self.holes as f32 / self.hole_maximum.max(1) as f32 * self.hole_dependence;
            if maximum < 0.0 {
                maximum = 0.0;
            }
        }
        let mut current = maximum;
        if self.rolling {
            current = 0.0;
        } else {
            let mut z = local[2] * 0.5 + 0.5;
            if z > 1.0 {
                z = 1.0;
            }
            let x = if local[0] < 0.0 {
                (z - 1.0) * local[0]
            } else {
                (1.0 - z) * local[0]
            };
            let c = match self.kind {
                Kind::Triangle => coefficients.triangle,
                Kind::Trapezoid => coefficients.trapezoid,
                _ => coefficients.square,
            };
            current *= c[0] + x * c[1] + z * c[2];
            if let Some(delta) = self.rolling_delta {
                current *= delta;
            }
        }
        ensure!(
            local
                .into_iter()
                .chain([current, maximum])
                .all(f32::is_finite),
            "sail drive overflow"
        );
        Ok(SailOutput {
            wind: local,
            current,
            maximum,
        })
    }
}
#[derive(Clone, Debug)]
pub struct Group {
    pub ship: u64,
    pub deleted: bool,
    pub current_set: i32,
    pub speed: f32,
    pub maximum: f32,
    pub reference: f32,
    pub speed_multiplier: f32,
    pub sails: Vec<Sail>,
}
impl Group {
    pub fn evaluate(
        &mut self,
        wind: [f32; 3],
        coefficients: Coefficients,
    ) -> Result<Vec<SailOutput>> {
        if self.deleted {
            return Ok(Vec::new());
        }
        ensure!(
            self.sails.len() <= 4096 && self.reference.is_finite(),
            "invalid sail group"
        );
        let outputs = self
            .sails
            .iter()
            .map(|s| s.evaluate(wind, coefficients))
            .collect::<Result<Vec<_>>>()?;
        let (mut current, mut maximum) = (0.0, 0.0);
        for o in &outputs {
            current += o.current;
            maximum += o.maximum;
        }
        let (speed, maximum) = if self.reference < 0.001 {
            (0.0, 0.0)
        } else {
            (current / self.reference, maximum / self.reference)
        };
        ensure!(
            [speed, maximum].into_iter().all(f32::is_finite),
            "sail group overflow"
        );
        self.speed = speed;
        self.maximum = maximum;
        Ok(outputs)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollRequest {
    pub group: usize,
    pub sail: usize,
    pub roll_up: bool,
}
/// Selected native case. A null destination has no effects; a missing group
/// writes the native fallback. SetRolling calls are returned in native order.
pub fn request_speed(
    groups: &mut [Group],
    ship: u64,
    output: Option<&mut f32>,
    request: f32,
    minimum: f32,
) -> Result<Vec<RollRequest>> {
    let Some(output) = output else {
        return Ok(Vec::new());
    };
    ensure!(
        groups.len() <= 4096 && request.is_finite() && minimum.is_finite(),
        "invalid sail speed request"
    );
    let Some(index) = groups.iter().position(|g| g.ship == ship) else {
        *output = if request > 0.3 { minimum } else { 0.0 };
        return Ok(Vec::new());
    };
    let g = &mut groups[index];
    if g.deleted {
        *output = 0.0;
        return Ok(Vec::new());
    }
    ensure!(
        g.sails.len() <= 4096
            && [g.speed, g.reference, g.speed_multiplier]
                .into_iter()
                .all(f32::is_finite),
        "invalid sail group state"
    );
    let speed = if g.current_set != 0 && g.speed < minimum {
        minimum
    } else {
        g.speed
    };
    let target = if request < 0.3 {
        0
    } else if request > 0.7 {
        2
    } else {
        1
    };
    let mut calls = Vec::new();
    if g.current_set != target {
        let mut folded = 0.0;
        for (sail, s) in g.sails.iter().enumerate() {
            ensure!(s.maximum.is_finite(), "invalid sail capacity");
            let roll_up = s.roll_type > target;
            calls.push(RollRequest {
                group: index,
                sail,
                roll_up,
            });
            if roll_up {
                folded += s.maximum;
            }
        }
        let multiplier = if folded > 0.0 {
            request / (folded / g.reference)
        } else {
            g.speed_multiplier
        };
        ensure!(multiplier.is_finite(), "sail multiplier overflow");
        g.speed_multiplier = multiplier;
        g.current_set = target;
    }
    *output = speed;
    Ok(calls)
}

/// Numeric SAILONE initialization over native hard-point order.
pub fn capacity(points: [[f32; 3]; 4], triangle: bool) -> Result<f32> {
    ensure!(
        points.into_iter().flatten().all(f32::is_finite),
        "invalid sail hard points"
    );
    let length = |v: [f32; 3]| libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    let (width, height) = if triangle {
        (
            length(std::array::from_fn(|i| points[2][i] - points[0][i])),
            length(std::array::from_fn(|i| points[1][i] - points[2][i])),
        )
    } else {
        (
            0.5 * length(std::array::from_fn(|i| {
                points[0][i] + points[2][i] - points[1][i] - points[3][i]
            })),
            0.5 * length(std::array::from_fn(|i| {
                points[0][i] + points[1][i] - points[2][i] - points[3][i]
            })),
        )
    };
    let maximum = if triangle {
        width * height * 0.5
    } else {
        width * height
    };
    ensure!(maximum.is_finite(), "sail capacity overflow");
    Ok(maximum)
}
#[cfg(test)]
mod tests;
