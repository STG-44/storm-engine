//! Numeric surface branches of original SEA::AttributeChanged.
//! Visual/fog/resource callbacks remain outside this scalar surface controller.
use super::*;
use storm_core::{AttributeId, AttributeStore};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub raw_amplitude: [f32; 2],
    pub max_height: f32,
    pub stopped: bool,
    pub speed_y: [f32; 2],
    pub movement_y: [f32; 2],
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub surface_attribute: bool,
    /// Parent isDone triggers Realize(0) before later branches, even when stopped.
    pub realize_zero_requested: bool,
}

fn vector(mut text: &str, mut previous: [f32; 3]) -> Result<[f32; 3]> {
    // Retain %f, %f, %f partial assignment and literal-comma behavior.
    // Decimal spellings are the supported scanf input subset. Reject hex,
    // incomplete exponents and non-finite results rather than guessing them.
    for slot in &mut previous {
        text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
        let b = text.as_bytes();
        let sign = usize::from(matches!(b.first(), Some(b'+' | b'-')));
        let special = text.get(sign..).unwrap_or("").to_ascii_lowercase();
        ensure!(
            !special.starts_with("nan") && !special.starts_with("inf"),
            "non-finite SEA move speed"
        );
        ensure!(
            !text
                .get(sign..)
                .is_some_and(|s| s.starts_with("0x") || s.starts_with("0X")),
            "hex SEA move speed is outside the checked decimal subset"
        );
        let mut end = sign;
        let mut digits = 0;
        while b.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
            digits += 1;
        }
        if b.get(end) == Some(&b'.') {
            end += 1;
            while b.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            break;
        }
        if matches!(b.get(end), Some(b'e' | b'E')) {
            end += 1;
            if matches!(b.get(end), Some(b'+' | b'-')) {
                end += 1;
            }
            let start = end;
            while b.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            ensure!(end > start, "incomplete SEA move speed exponent");
        }
        *slot = text[..end].parse::<f32>()?;
        ensure!(slot.is_finite(), "non-finite SEA move speed");
        text = &text[end..];
        if let Some(rest) = text.strip_prefix(',') {
            text = rest;
        } else {
            break;
        }
    }
    Ok(previous)
}

impl Sea {
    /// Enable original attribute-driven behavior from the current edit defaults.
    pub fn enable_weather(&mut self) {
        if self.weather.is_none() {
            self.weather = Some(Weather {
                raw_amplitude: self.surface.parameters.amplitude,
                max_height: 20.0,
                stopped: false,
                speed_y: [0.0; 2],
                movement_y: [0.0; 2],
            });
        }
    }
    pub fn weather(&self) -> Option<&Weather> {
        self.weather.as_ref()
    }

    /// The caller supplies the entity's original AttributesPointer binding.
    /// Attribute writes themselves belong to the caller's store; only controller
    /// state is transactional if a checked input is rejected.
    pub fn attribute_changed(
        &mut self,
        store: &AttributeStore,
        root: AttributeId,
        changed: AttributeId,
    ) -> Result<Change> {
        ensure!(
            self.weather.is_some(),
            "SEA weather attributes are not enabled"
        );
        ensure!(store.get(root).is_some(), "stale SEA attribute root");
        let mut cursor = Some(changed);
        let mut belongs = false;
        while let Some(id) = cursor {
            if id == root {
                belongs = true;
                break;
            }
            cursor = store
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("stale SEA attribute"))?
                .parent();
        }
        ensure!(
            belongs && changed != root,
            "SEA callback requires a descendant of its bound root"
        );
        let parent = store.get(changed).unwrap().parent().unwrap();
        let name = store.name(changed).unwrap().to_ascii_lowercase();
        let parent = store.name(parent).unwrap().to_ascii_lowercase();
        let mut candidate = self.clone();
        let mut result = Change::default();
        if parent == "isdone" {
            candidate.advance(0)?;
            result.realize_zero_requested = true;
        }
        let p = &mut candidate.surface.parameters;
        let w = candidate.weather.as_mut().unwrap();
        let f = || store.as_float(changed, None, 0.0);
        if parent == "sea2" {
            let index = match name.as_str() {
                "amp1" | "animspeed1" | "scale1" | "movespeed1" => Some(0),
                "amp2" | "animspeed2" | "scale2" | "movespeed2" => Some(1),
                _ => None,
            };
            if let Some(i) = index {
                result.surface_attribute = true;
                match name.as_str() {
                    "amp1" | "amp2" => w.raw_amplitude[i] = f(),
                    "animspeed1" | "animspeed2" => p.animation_speed[i] = f(),
                    "scale1" | "scale2" => p.scale[i] = f(),
                    _ => {
                        let v = vector(
                            store.value(changed, None).unwrap_or(""),
                            [p.move_speed[i][0], w.speed_y[i], p.move_speed[i][1]],
                        )?;
                        p.move_speed[i] = [v[0], v[2]];
                        w.speed_y[i] = v[1];
                    }
                }
            }
        } else if parent != "fog" {
            match name.as_str() {
                "stop" => {
                    w.stopped = store.as_dword(root, Some("Stop"), 0) != 0;
                    result.surface_attribute = true;
                }
                "maxseaheight" => {
                    w.max_height = store.as_float(root, Some("MaxSeaHeight"), 50.0);
                    let sum = w.raw_amplitude[0] + w.raw_amplitude[1];
                    let scale = if w.max_height >= sum {
                        1.0
                    } else {
                        w.max_height / sum
                    };
                    p.amplitude = w.raw_amplitude.map(|v| v * scale);
                    result.surface_attribute = true;
                }
                "maxseadistance" => {
                    p.max_distance = store.as_float(root, Some("MaxSeaDistance"), 0.0);
                    result.surface_attribute = true;
                }
                _ => {}
            }
        }
        ensure!(
            w.raw_amplitude
                .into_iter()
                .chain(w.speed_y)
                .chain([w.max_height])
                .all(f32::is_finite),
            "non-finite SEA weather state"
        );
        p.validate()?;
        *self = candidate;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
