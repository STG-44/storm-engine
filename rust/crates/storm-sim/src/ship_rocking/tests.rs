use super::*;
use serde_json::{Value as Json, json};
use std::collections::VecDeque;

struct Surface {
    kind: u32,
    base: f32,
    math: VecDeque<[u32; 3]>,
    queries: Vec<[u32; 3]>,
    observed: Vec<[u32; 3]>,
}
impl Surface {
    fn math(&mut self, kind: u32, value: f32) -> f32 {
        let expected = self.math.pop_front().expect("unrecorded native math call");
        assert_eq!([kind, value.to_bits()], [expected[0], expected[1]]);
        let actual = match kind {
            0 => libm::sinf(value),
            1 => libm::cosf(value),
            2 => libm::atanf(value),
            _ => unreachable!(),
        };
        let ordered = |bits: u32| {
            if bits >> 31 == 0 {
                bits | 0x80000000
            } else {
                !bits
            }
        };
        assert!(
            ordered(actual.to_bits()).abs_diff(ordered(expected[2])) <= 1,
            "software/native scalar divergence: {kind} {value} {actual} {}",
            f32::from_bits(expected[2])
        );
        assert_eq!(actual.to_bits() >> 31, expected[2] >> 31);
        if actual == 0.0 || f32::from_bits(expected[2]) == 0.0 {
            assert_eq!(actual.to_bits(), expected[2]);
        }
        self.observed.push(expected);
        f32::from_bits(expected[2])
    }
}
impl SeaSurface for Surface {
    fn wave_xz(&mut self, x: f32, z: f32) -> Result<f32> {
        let mut value = self.base + 0.03125 * x - 0.0625 * z + 0.0005 * x * z;
        if self.kind == 1 {
            value = self.base;
        }
        if self.kind == 2 {
            value += if self.queries.len() % 2 == 1 {
                -0.375
            } else {
                0.375
            };
        }
        self.queries
            .push([x.to_bits(), z.to_bits(), value.to_bits()]);
        Ok(value)
    }
    fn sin(&mut self, x: f32) -> f32 {
        self.math(0, x)
    }
    fn cos(&mut self, x: f32) -> f32 {
        self.math(1, x)
    }
    fn atan(&mut self, x: f32) -> f32 {
        self.math(2, x)
    }
}
#[test]
fn all_native_rocking_grids_queries_and_complete_outputs_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/ship_rocking/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 96);
    for case in data["cases"].as_array().unwrap() {
        let f = |k: &str| case[k].as_f64().unwrap() as f32;
        let mut rocking = ShipRocking::new([f("width"), f("length")]);
        rocking.parameters.offsets = [f("offsetx"), f("offsetz")];
        rocking.parameters.height_factor = f("height");
        rocking.parameters.angle_factor = f("angular");
        rocking.heights = [[42.0; GRID_SIDE]; GRID_SIDE];
        let mut input = RockingInput {
            position: [3.0, 2.0, -5.0],
            angles: [0.125, f("yaw"), -0.25],
            previous_angles: [-0.125, f("yaw") + 0.0625, 0.375],
        };
        for (step, expected) in case["snapshots"].as_array().unwrap().iter().enumerate() {
            let mut sea = Surface {
                kind: case["kind"].as_u64().unwrap() as u32,
                base: step as f32 * 0.25,
                math: serde_json::from_value(expected["math"].clone()).unwrap(),
                queries: Vec::new(),
                observed: Vec::new(),
            };
            let surface: Option<&mut dyn SeaSurface> =
                case["sea"].as_bool().unwrap().then_some(&mut sea);
            let output = rocking.advance(input, f("dt"), surface).unwrap();
            assert_eq!(
                json!({"height":output.height.to_bits(),"angles":output.angles.map(f32::to_bits),
                "heights":rocking.heights.map(|r| r.map(f32::to_bits)),"queries":sea.queries,"math":sea.observed}),
                *expected,
                "case {} step {step}",
                case["id"]
            );
            assert!(sea.math.is_empty());
            input.position[1] = output.height;
            input.position[0] += 0.375;
            input.position[2] -= 0.25;
            input.angles = output.angles;
            input.previous_angles = output.angles;
        }
    }
}

struct Plane {
    samples: usize,
    invalid: Option<usize>,
}
impl SeaSurface for Plane {
    fn wave_xz(&mut self, x: f32, z: f32) -> Result<f32> {
        self.samples += 1;
        Ok(if self.invalid == Some(self.samples) {
            f32::NAN
        } else {
            2.0 + 0.125 * x - 0.25 * z
        })
    }
}
fn input() -> RockingInput {
    RockingInput {
        position: [0.0, 7.0, 0.0],
        angles: [0.0; 3],
        previous_angles: [0.0; 3],
    }
}
#[test]
fn plane_signs_time_cap_and_absent_sea_follow_native_contracts() {
    let mut rocking = ShipRocking::new([6.0, 16.0]);
    let mut plane = Plane {
        samples: 0,
        invalid: None,
    };
    let mut capped = rocking.clone();
    let a = rocking.advance(input(), 0.1, Some(&mut plane)).unwrap();
    let b = capped.advance(input(), 3.0, Some(&mut plane)).unwrap();
    assert_eq!(a, b);
    assert!((a.height - 2.0).abs() < 1e-6);
    assert!((a.angles[0] - libm::atanf(0.25)).abs() < 1e-6);
    assert!((a.angles[2] - libm::atanf(0.125)).abs() < 1e-6);
    assert_eq!(plane.samples, 72);
    let saved = rocking.heights;
    let mut i = input();
    i.previous_angles = [0.1, 0.2, 0.3];
    let absent = rocking.advance(i, 1.0, None).unwrap();
    assert_eq!(
        absent,
        RockingOutput {
            height: 7.0,
            angles: i.previous_angles
        }
    );
    assert_eq!(rocking.heights, saved);
    let zero = rocking.advance(i, 0.0, Some(&mut plane)).unwrap();
    assert_eq!(
        zero,
        RockingOutput {
            height: 7.0,
            angles: i.previous_angles
        }
    );
    assert_eq!(plane.samples, 108); // Zero delta still samples the original grid.
}
#[test]
fn invalid_geometry_and_bad_sea_do_not_commit_partial_grids() {
    let mut rocking = ShipRocking::new([6.0, 16.0]);
    let mut plane = Plane {
        samples: 0,
        invalid: Some(19),
    };
    let saved = rocking.clone();
    assert!(rocking.advance(input(), 0.016, Some(&mut plane)).is_err());
    assert_eq!(rocking, saved);
    for delta in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(rocking.advance(input(), delta, Some(&mut plane)).is_err());
    }
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        rocking.parameters.size[0] = bad;
        assert!(rocking.advance(input(), 0.016, Some(&mut plane)).is_err());
    }
    assert_eq!(plane.samples, 19);
}

#[test]
fn diagnostic_surface_uses_the_rendered_wave_coefficients_with_portable_scalar_math() {
    let waves = crate::Waves {
        height_m: 1.5,
        direction: 1.2,
    };
    let mut sea = DiagnosticSea::new(waves).unwrap();
    for i in 0..64 {
        let x = i as f32 * 0.375;
        let z = i as f32 * -0.25;
        sea.seconds = i as f32 * 0.125;
        let value = sea.wave_xz(x, z).unwrap();
        let expected = waves
            .sample(
                glam::DVec2::new(f64::from(x), f64::from(z)),
                f64::from(sea.seconds),
            )
            .0;
        assert!((f64::from(value) - expected).abs() < 2e-6);
    }
    sea.seconds = f32::NAN;
    assert!(sea.wave_xz(0.0, 0.0).is_err());
}
