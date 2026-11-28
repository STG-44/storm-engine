use super::super::*;
use serde_json::{Value as Json, json};

struct Effects {
    available: bool,
    create_mask: u32,
    valid_mask: u32,
    sound: i32,
    next: u64,
    events: Vec<String>,
}
fn position(p: [f32; 3]) -> String {
    p.map(f32::to_bits).map(|n| n.to_string()).join(",")
}
impl FireEffects for Effects {
    fn has_particles(&self) -> bool {
        self.available
    }
    fn create_particle(
        &mut self,
        _: ParticleKind,
        name: &str,
        _: usize,
        p: [f32; 3],
    ) -> Option<ParticleId> {
        self.next += 1;
        self.events
            .push(format!("create:{}:{name}:{}", self.next, position(p)));
        (self.create_mask & (1 << ((self.next - 1) % 2)) != 0).then_some(ParticleId(self.next))
    }
    fn validate_particle(&mut self, id: ParticleId) -> bool {
        let valid = self.valid_mask & (1 << ((id.0 - 1) % 2)) != 0;
        self.events
            .push(format!("validate:{}:{}", id.0, u8::from(valid)));
        valid
    }
    fn pause_particle(&mut self, id: ParticleId) {
        self.events.push(format!("pause:{}", id.0));
    }
    fn move_particle(&mut self, id: ParticleId, p: [f32; 3]) {
        self.events.push(format!("move:{}:{}", id.0, position(p)));
    }
    fn play_sound(&mut self, name: &str, p: [f32; 3]) -> i32 {
        self.events.push(format!("play:{name}:{}", position(p)));
        self.sound
    }
    fn stop_sound(&mut self, id: i32) {
        self.events.push(format!("stop:{id}"));
    }
    fn move_sound(&mut self, id: i32, p: [f32; 3]) {
        self.events.push(format!("sound_move:{id}:{}", position(p)));
    }
}
fn fixture() -> Json {
    serde_json::from_str(include_str!("../../../reference/fire/cases.json")).unwrap()
}
fn snapshot(fire: Option<&FirePlace>, effects: &mut Effects) -> Json {
    let state=fire.map(|f|json!({"active":f.active,"remaining_bits":f.remaining.map(f32::to_bits),"owner":f.owner,
        "sound":f.sound,"smoke":f.smoke.map_or(0,|id|id.0),"fire":f.fire.map_or(0,|id|id.0),"names":f.names}));
    json!({"state":state,"events":std::mem::take(&mut effects.events)})
}
#[test]
fn unedited_fireplace_lifecycle_matches_480_complete_state_and_service_snapshots() {
    let data = fixture();
    let cases = data["lifecycle"].as_array().unwrap();
    assert_eq!(cases.len(), 60);
    for case in cases {
        let mut effects = Effects {
            available: case["available"].as_bool().unwrap(),
            create_mask: case["create_mask"].as_u64().unwrap() as u32,
            valid_mask: case["valid_mask"].as_u64().unwrap() as u32,
            sound: case["sound"].as_i64().unwrap() as i32,
            next: 0,
            events: vec![],
        };
        let mut fire = FirePlace::new();
        let mut snapshots = vec![snapshot(Some(&fire), &mut effects)];
        fire.run(
            "smoke",
            "fire",
            7,
            "sound",
            f32::from_bits(case["duration_bits"].as_u64().unwrap() as u32),
            0,
            [1.0, 2.0, 3.0],
            &mut effects,
        );
        snapshots.push(snapshot(Some(&fire), &mut effects));
        fire.run(
            "ignored",
            "ignored",
            19,
            "ignored",
            99.0,
            0,
            [1.0, 2.0, 3.0],
            &mut effects,
        );
        snapshots.push(snapshot(Some(&fire), &mut effects));
        fire.execute(
            f32::from_bits(case["delta_bits"].as_u64().unwrap() as u32),
            [4.0, 5.0, 6.0],
            &mut effects,
        );
        snapshots.push(snapshot(Some(&fire), &mut effects));
        if case["lost"].as_bool().unwrap() {
            effects.available = false;
        }
        fire.stop(&mut effects);
        snapshots.push(snapshot(Some(&fire), &mut effects));
        fire.run(
            "restart-smoke",
            "restart-fire",
            23,
            "restart-sound",
            2.0,
            0,
            [4.0, 5.0, 6.0],
            &mut effects,
        );
        snapshots.push(snapshot(Some(&fire), &mut effects));
        fire.execute(0.5, [7.0, 8.0, 9.0], &mut effects);
        snapshots.push(snapshot(Some(&fire), &mut effects));
        // The native destructor calls Stop. Production ShipFire owns the effects
        // port and invokes this same operation from Drop (tested separately).
        fire.stop(&mut effects);
        snapshots.push(snapshot(None, &mut effects));
        assert_eq!(json!(snapshots), case["expected"], "{case}");
    }
}
#[test]
fn native_ship_activation_and_safe_delete_match_service_order_and_cleanup() {
    let data = fixture();
    assert_eq!(
        data["constants"]["MSG_SHIP_ACTIVATE_FIRE_PLACE"],
        MSG_SHIP_ACTIVATE_FIRE_PLACE
    );
    assert_eq!(
        data["constants"]["MSG_SHIP_SAFE_DELETE"],
        MSG_SHIP_SAFE_DELETE
    );
    let cases = data["messages"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for case in cases {
        let count = case["count"].as_u64().unwrap() as usize;
        let index = case["index"].as_i64().unwrap() as i32;
        let effects = Effects {
            available: true,
            create_mask: 3,
            valid_mask: 3,
            sound: 7,
            next: 0,
            events: vec![],
        };
        let mut ship = ShipFire::new(vec![[1.0, 2.0, 3.0]; count], effects);
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
        let result = ship.process_message(&mut activate(index));
        if result.is_ok() && count > 1 && index == 2 {
            ship.process_message(&mut activate(0)).unwrap();
        }
        let active = ship
            .queries
            .fire_places()
            .iter()
            .filter(|s| s.state.is_active())
            .count();
        let events = std::mem::take(&mut ship.effects.events);
        let clear = ship
            .process_message(
                &mut Message::new("l", vec![Value::Long(MSG_SHIP_SAFE_DELETE)]).unwrap(),
            )
            .unwrap();
        let actual = json!({"error":result.is_err(),"active":active,"events":events,"clear_result":clear,"remaining_places":ship.queries.fire_places().len(),"clear_events":ship.effects.events});
        assert_eq!(actual, case["expected"], "{case}");
    }
}
