use super::*;
use crate::{Helm, RigState, SailMode, Sailing, ShipConfig, Wind};
use std::{cell::RefCell, rc::Rc};
use storm_core::{ScriptScalar, ScriptVariable, ScriptVariableAccess, Value};

struct Output(Rc<RefCell<ScriptScalar>>);
impl ScriptVariableAccess for Output {
    fn read(&self) -> Result<ScriptScalar> {
        Ok(self.0.borrow().clone())
    }
    fn write(&self, value: ScriptScalar) -> Result<()> {
        let nan = matches!(value, ScriptScalar::Float(n) if n.is_nan());
        *self.0.borrow_mut() = value;
        anyhow::ensure!(!nan, "NAN ERROR");
        Ok(())
    }
}
fn set(value: f32) -> Message {
    Message::new(
        "lf",
        vec![Value::Long(MSG_SHIP_SET_SAIL_STATE), Value::Float(value)],
    )
    .unwrap()
}
fn simulation() -> Sailing {
    Sailing::new(
        ShipConfig::default(),
        Wind {
            speed_mps: 12.0,
            direction: 0.0,
        },
        0.0,
    )
    .unwrap()
}

#[test]
fn retained_ship_sail_cases_preserve_float_bits_retype_output_and_return_zero() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../reference/ship_sails/cases.json")).unwrap();
    assert_eq!(
        fixture["constants"]["MSG_SHIP_SET_SAIL_STATE"],
        MSG_SHIP_SET_SAIL_STATE
    );
    assert_eq!(
        fixture["constants"]["MSG_SHIP_GET_SAIL_STATE"],
        MSG_SHIP_GET_SAIL_STATE
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    for case in cases {
        let mut state = ShipSailState::default();
        let expected = &case["expected"];
        if case["set"].as_bool().unwrap() {
            let result = state
                .process_message(&mut set(f32::from_bits(
                    case["bits"].as_u64().unwrap() as u32
                )))
                .unwrap();
            assert_eq!(result, expected["set_result"].as_u64().unwrap());
        }
        assert_eq!(
            u64::from(state.get().to_bits()),
            expected["state_bits"].as_u64().unwrap()
        );
        let output = Rc::new(RefCell::new(ScriptScalar::Int(19)));
        let variable = ScriptVariable::new(Output(output.clone()));
        let mut message = Message::new(
            "le",
            vec![
                Value::Long(MSG_SHIP_GET_SAIL_STATE),
                Value::ScriptVariable(variable),
            ],
        )
        .unwrap();
        let result = state.process_message(&mut message);
        assert_eq!(result.is_err(), expected["get_error"].as_bool().unwrap());
        if let Ok(result) = result {
            assert_eq!(result, expected["get_result"].as_u64().unwrap());
        }
        let ScriptScalar::Float(value) = *output.borrow() else {
            panic!("native setter must retype output")
        };
        assert_eq!(
            u64::from(value.to_bits()),
            expected["output_bits"].as_u64().unwrap()
        );
    }
}
#[test]
fn malformed_payloads_do_not_write_state_and_unported_codes_are_explicit() {
    let mut state = ShipSailState::default();
    state.set(0.5);
    for mut message in [
        Message::new("l", vec![Value::Long(MSG_SHIP_SET_SAIL_STATE)]).unwrap(),
        Message::new(
            "ll",
            vec![Value::Long(MSG_SHIP_SET_SAIL_STATE), Value::Long(1)],
        )
        .unwrap(),
        Message::new(
            "lf",
            vec![Value::Long(MSG_SHIP_GET_SAIL_STATE), Value::Float(1.0)],
        )
        .unwrap(),
        Message::new("l", vec![Value::Long(50300)]).unwrap(),
    ] {
        assert!(state.process_message(&mut message).is_err());
        assert_eq!(state.get(), 0.5);
    }
}
#[test]
fn native_request_changes_real_cloth_gradually_and_helm_uses_the_same_state() {
    let mut sim = simulation();
    // A double just outside a threshold can round onto its float32 boundary.
    // Initial selection and zero-time application must use the same precision.
    for setting in [0.299999999, 0.700000001] {
        sim.set_helm(Helm {
            sail: setting,
            rudder: 0.0,
        })
        .unwrap();
        let mode = sim.sail_mode();
        sim.advance(0.0).unwrap();
        assert_eq!(sim.sail_mode(), mode);
    }
    sim.process_message(&mut set(0.4)).unwrap();
    assert_eq!(sim.sail_request().to_bits(), 0.4f32.to_bits());
    assert_eq!(sim.rig(), RigState::for_mode(SailMode::Full));
    sim.advance(0.0).unwrap();
    assert_eq!(sim.sail_mode(), SailMode::Normal);
    assert_eq!(sim.helm().sail, 0.5);
    sim.advance(2.0).unwrap();
    assert!((sim.rig().full_only - 0.5).abs() < 1e-12);
    assert_eq!(sim.rig().normal, 1.0);
    assert_eq!(sim.sail_request().to_bits(), 0.4f32.to_bits());
    sim.set_helm(Helm {
        sail: 0.0,
        rudder: 0.0,
    })
    .unwrap();
    assert_eq!(sim.sail_request(), 0.0);
    sim.advance(4.0).unwrap();
    assert_eq!(sim.rig(), RigState::for_mode(SailMode::Furled));
}
#[test]
fn solver_rejects_invalid_raw_commands_without_clamping_or_advancing_and_can_recover() {
    let mut sim = simulation();
    let state = sim.state();
    for value in [-1.0, 1.5, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        sim.process_message(&mut set(value)).unwrap();
        assert!(sim.advance(1.0).is_err());
        assert_eq!(sim.sail_request().to_bits(), value.to_bits());
        assert_eq!(sim.state(), state);
        assert_eq!(sim.time(), 0.0);
        assert_eq!(sim.rig(), RigState::for_mode(SailMode::Full));
    }
    sim.process_message(&mut set(0.0)).unwrap();
    sim.advance(4.0).unwrap();
    assert_eq!(sim.rig(), RigState::for_mode(SailMode::Furled));
}
