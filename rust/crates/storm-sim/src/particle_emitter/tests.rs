use super::*;
use serde_json::{Value as Json, json};
#[derive(Default)]
struct Port {
    base: f32,
    slope: f32,
    fail_every: usize,
    allocations: usize,
    rates: Vec<[u32; 4]>,
    births: Vec<[u32; 6]>,
}
impl BirthPort for Port {
    fn rate(&mut self, index: usize, time: f32, life: f32) -> f32 {
        let rate = if index == 0 {
            self.base + self.slope * time
        } else {
            3.0
        };
        self.rates
            .push([index as u32, time.to_bits(), life.to_bits(), rate.to_bits()]);
        rate
    }
    fn birth(&mut self, b: Birth) -> bool {
        self.allocations += 1;
        let ok = self.fail_every == 0 || !self.allocations.is_multiple_of(self.fail_every);
        self.births.push([
            b.particle_type as u32,
            b.emitter_time.to_bits(),
            b.emitter_lifetime.to_bits(),
            b.matrix_blend.to_bits(),
            u32::from(ok),
            match b.kind {
                Kind::Billboard => 1,
                Kind::Model => 2,
            },
        ]);
        ok
    }
}
fn snapshot(e: &Emitter, port: &mut Port, count: usize) -> Json {
    json!({"elapsed":e.elapsed.to_bits(),"stopped":e.stopped,"count":count,"types":e.types.iter().map(|t|[t.remainder.to_bits(),t.active]).collect::<Vec<_>>(),"rates":std::mem::take(&mut port.rates),"births":std::mem::take(&mut port.births)})
}
#[test]
fn native_birth_time_and_pause_match_complete_cpp_traces() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/particle_emitter/cases.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 35);
    for case in cases {
        let f = |s: &str| case[s].as_f64().unwrap() as f32;
        let b = |s: &str| case[s].as_bool().unwrap();
        let mut e = Emitter::new(
            f("lifetime"),
            b("looping"),
            vec![
                ParticleType::new(Kind::Billboard, case["maximum"].as_u64().unwrap() as u32),
                ParticleType::new(Kind::Model, case["maximum"].as_u64().unwrap() as u32),
            ],
        );
        e.elapsed = f("elapsed");
        e.stopped = b("stopped");
        e.enabled = b("enabled");
        e.attached = b("attached");
        e.types[0].enabled = b("type_enabled");
        e.types[0].remainder = f("remainder");
        let mut port = Port {
            base: f("base"),
            slope: f("slope"),
            fail_every: case["fail_every"].as_u64().unwrap() as usize,
            ..Default::default()
        };
        let mut states = vec![snapshot(&e, &mut port, 1)];
        let mut system = System {
            paused: false,
            emitters: vec![e],
        };
        for (index, delta) in data["deltas"].as_array().unwrap().iter().enumerate() {
            system.paused = index == 2;
            let count = system
                .execute(delta.as_f64().unwrap() as f32, &mut port)
                .unwrap();
            let e = &mut system.emitters[0];
            if index % 2 == 0 {
                for t in 0..e.types.len() {
                    if e.types[t].active > 0 {
                        e.release(t).unwrap();
                    }
                }
            }
            states.push(snapshot(e, &mut port, count));
        }
        assert_eq!(json!(states), case["expected"], "{case}");
    }
}
#[test]
fn full_capacity_drops_birth_attempts_without_a_delayed_burst() {
    let mut e = Emitter::new(10.0, true, vec![ParticleType::new(Kind::Billboard, 1)]);
    let mut p = Port {
        base: 32.0,
        ..Default::default()
    };
    e.execute(1.0, &mut p).unwrap();
    assert_eq!(p.births.len(), 1);
    assert_eq!(e.types[0].remainder, 0.0);
    e.release(0).unwrap();
    e.execute(0.0, &mut p).unwrap();
    assert_eq!(p.births.len(), 1);
    e.execute(0.03125, &mut p).unwrap();
    assert_eq!(p.births.len(), 2);
}
#[test]
fn checked_emission_budgets_and_counter_release_reject_unbounded_work() {
    let mut e = Emitter::new(1.0, true, vec![ParticleType::new(Kind::Billboard, 1)]);
    let mut p = Port::default();
    assert!(e.execute(f32::NAN, &mut p).is_err());
    assert!(e.release(0).is_err());
    assert!(e.release(1).is_err());
    p.base = f32::INFINITY;
    assert!(e.execute(1.0, &mut p).is_err());
    let mut e = Emitter::new(1.0, true, vec![ParticleType::new(Kind::Billboard, 0)]);
    p.base = 1_000_000_000.0;
    assert!(e.execute(1.0, &mut p).is_err());
}
