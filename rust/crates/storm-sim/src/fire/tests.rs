use super::*;
mod reference;
#[derive(Default)]
struct Effects {
    present: bool,
    created: usize,
    paused: usize,
    moved: usize,
    plays: usize,
    stops: usize,
}
impl FireEffects for Effects {
    fn has_particles(&self) -> bool {
        self.present
    }
    fn create_particle(
        &mut self,
        _: ParticleKind,
        _: &str,
        _: usize,
        _: [f32; 3],
    ) -> Option<ParticleId> {
        self.created += 1;
        Some(ParticleId(self.created as u64))
    }
    fn validate_particle(&mut self, _: ParticleId) -> bool {
        true
    }
    fn pause_particle(&mut self, _: ParticleId) {
        self.paused += 1;
    }
    fn move_particle(&mut self, _: ParticleId, _: [f32; 3]) {
        self.moved += 1;
    }
    fn play_sound(&mut self, _: &str, _: [f32; 3]) -> i32 {
        self.plays += 1;
        7
    }
    fn stop_sound(&mut self, _: i32) {
        self.stops += 1;
    }
    fn move_sound(&mut self, _: i32, _: [f32; 3]) {}
}
#[test]
fn active_run_is_ignored_zero_time_stays_active_and_stop_retains_duration_and_names() {
    let mut fire = FirePlace::new();
    let mut effects = Effects {
        present: true,
        ..Default::default()
    };
    assert_eq!(fire.remaining(), None);
    assert_eq!(fire.owner(), -1);
    fire.run("smoke", "fire", 19, "sound", 1.0, 0, [0.0; 3], &mut effects);
    fire.run(
        "other",
        "other",
        27,
        "other",
        99.0,
        0,
        [0.0; 3],
        &mut effects,
    );
    assert_eq!(effects.created, 2);
    assert_eq!(effects.plays, 1);
    assert_eq!(fire.owner(), 19);
    fire.execute(1.0, [1.0; 3], &mut effects);
    assert!(fire.is_active());
    assert_eq!(fire.remaining(), Some(0.0));
    fire.execute(0.125, [2.0; 3], &mut effects);
    assert!(!fire.is_active());
    assert_eq!(fire.remaining(), Some(-0.125));
    assert_eq!(
        fire.names().each_ref().map(String::as_str),
        ["smoke", "fire", "sound"]
    );
    assert_eq!(fire.owner(), -1);
    assert_eq!(effects.paused, 2);
    assert_eq!(effects.stops, 1);
    fire.stop(&mut effects);
    assert_eq!(effects.stops, 1);
}
#[test]
fn absent_particle_service_prevents_activation_and_raw_nonfinite_durations_are_retained() {
    let mut fire = FirePlace::new();
    let mut effects = Effects::default();
    fire.run("s", "f", 7, "a", 1.0, 0, [0.0; 3], &mut effects);
    assert!(!fire.is_active());
    assert_eq!(effects.plays, 0);
    assert_eq!(fire.remaining(), None);
    effects.present = true;
    for duration in [f32::NAN, f32::INFINITY, -1.0] {
        fire.run("s", "f", 7, "a", duration, 0, [0.0; 3], &mut effects);
        assert_eq!(fire.remaining().unwrap().to_bits(), duration.to_bits());
        fire.execute(0.0, [0.0; 3], &mut effects);
        assert_eq!(fire.is_active(), duration >= 0.0 || duration.is_nan());
        fire.stop(&mut effects);
    }
}
impl FireLocator for [f32; 3] {
    fn fire_position(&self) -> [f32; 3] {
        *self
    }
}
#[test]
fn message_activation_checks_fields_before_mutation_and_safe_delete_releases_records() {
    let mut ship = ShipFire::new(
        vec![[0.0; 3]],
        Effects {
            present: true,
            ..Default::default()
        },
    );
    let activate = |index| {
        Message::new(
            "llsssfl",
            vec![
                Value::Long(MSG_SHIP_ACTIVATE_FIRE_PLACE),
                Value::Long(index),
                Value::String("smoke".into()),
                Value::String("fire".into()),
                Value::String("sound".into()),
                Value::Float(1.0),
                Value::Long(7),
            ],
        )
        .unwrap()
    };
    for index in [-1, 1, i32::MIN] {
        assert!(ship.process_message(&mut activate(index)).is_err());
    }
    let fields = vec![
        Value::Long(MSG_SHIP_ACTIVATE_FIRE_PLACE),
        Value::Long(0),
        Value::String("smoke".into()),
        Value::String("fire".into()),
        Value::String("sound".into()),
        Value::Float(1.0),
    ];
    assert!(
        ship.process_message(&mut Message::new("llsssf", fields.clone()).unwrap())
            .is_err()
    );
    let mut wrong_type = fields;
    wrong_type[5] = Value::String("duration".into());
    wrong_type.push(Value::Long(7));
    assert!(
        ship.process_message(&mut Message::new("llssssl", wrong_type).unwrap())
            .is_err()
    );
    assert_eq!(ship.effects.created, 0);
    assert_eq!(ship.process_message(&mut activate(0)).unwrap(), 0);
    assert_eq!(ship.effects.created, 2);
    assert_eq!(
        ship.process_message(
            &mut Message::new("l", vec![Value::Long(MSG_SHIP_SAFE_DELETE)]).unwrap()
        )
        .unwrap(),
        0
    );
    assert!(ship.queries.fire_places().is_empty());
    assert_eq!(ship.effects.paused, 2);
    assert_eq!(ship.effects.stops, 1);
    assert!(ship.process_message(&mut activate(0)).is_err());
}
