use super::*;
use serde_json::{Value as Json, json};
fn snapshot(p: ShipParameters, s: MotionState, m: MountedParameters, calls: &Json) -> Json {
    let floats = [
        p.speed_rate,
        p.turn_rate,
        p.weight,
        p.speed_depend_weight,
        p.sub_sea_depend_weight,
        p.turn_depend_weight,
        p.turn_depend_speed,
        p.waterline,
    ]
    .map(f32::to_bits);
    let mount = [
        m.length,
        m.width,
        m.load,
        m.nominal_speed_z,
        m.nominal_speed_y,
        m.inertia_factor,
        s.mass_inertia,
        m.weight,
        m.gravity,
    ]
    .map(f32::to_bits);
    json!({"integers":[p.class,p.crew_min,p.crew_max,p.capacity],"floats":floats,"inertia":[s.inertia_accel.map(f32::to_bits),s.inertia_brake.map(f32::to_bits)],"mount":mount,"calls":calls})
}
#[test]
fn loaded_parameters_and_numeric_mount_match_complete_native_snapshots() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/ship_parameters/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 64);
    let names = [
        "Class",
        "SpeedRate",
        "TurnRate",
        "MinCrew",
        "MaxCrew",
        "Capacity",
        "Weight",
        "SpeedDependWeight",
        "SubSeaDependWeight",
        "TurnDependWeight",
        "TurnDependSpeed",
        "WaterLine",
        "InertiaAccelerationY",
        "InertiaBrakingY",
        "InertiaAccelerationX",
        "InertiaBrakingX",
        "InertiaAccelerationZ",
        "InertiaBrakingZ",
    ];
    for c in data["cases"].as_array().unwrap() {
        let f = |k: &str| c[k].as_f64().unwrap() as f32;
        let mut store = AttributeStore::default();
        let root = store.create_root().unwrap();
        if !c["absent"].as_bool().unwrap() {
            for (name, value) in [
                ("Class", c["cls"].as_u64().unwrap() as u32),
                ("MinCrew", u32::MAX),
                ("MaxCrew", 120),
                ("Capacity", c["capacity"].as_i64().unwrap() as u32),
            ] {
                store.set_dword(root, Some(name), value).unwrap();
            }
            for (name, value) in [
                ("SpeedRate", f("speed")),
                ("TurnRate", f("turn")),
                ("Weight", f("weight")),
                ("SpeedDependWeight", 0.125),
                ("SubSeaDependWeight", -0.25),
                ("TurnDependWeight", 0.5),
                ("TurnDependSpeed", 0.75),
                ("WaterLine", f("waterline")),
                ("InertiaAccelerationY", f("scale")),
                ("InertiaBrakingY", -f("scale")),
                ("InertiaAccelerationX", 2.0 * f("scale")),
                ("InertiaBrakingX", -2.0 * f("scale")),
                ("InertiaAccelerationZ", 3.0 * f("scale")),
                ("InertiaBrakingZ", -3.0 * f("scale")),
            ] {
                store
                    .set_attribute(root, name, Some(&value.to_string()))
                    .unwrap();
            }
        }
        let mut state = MotionState {
            position: [3.0, 7.0, -5.0],
            mass_inertia: 7.0,
            ..Default::default()
        };
        let p = ShipParameters::load(&store, root).unwrap();
        p.apply_inertia(&mut state).unwrap();
        let old = MountedParameters {
            gravity: 0.0,
            length: 99.0,
            width: 88.0,
            load: 6.0,
            nominal_speed_z: 4.0,
            nominal_speed_y: 3.0,
            inertia_factor: 8.0,
            mass_inertia: 7.0,
            weight: 5.0,
        };
        let rows = c["snapshots"].as_array().unwrap();
        assert_eq!(rows[0]["calls"], json!(names));
        assert_eq!(snapshot(p, state, old, &rows[0]["calls"]), rows[0]);
        let mut placement = ShipPlacement {
            immersion: 0.75,
            waterline: 9.0,
        };
        let mounted = p
            .configure(&mut state, &mut placement, [f("x"), f("y"), f("z")])
            .unwrap();
        assert_eq!(snapshot(p, state, mounted, &json!([])), rows[1]);
        assert_eq!(placement.waterline, p.waterline);
        assert_eq!(placement.immersion, 0.75);
        assert_eq!(state.position, [3.0, 7.0, -5.0]);
        if let Some(id) = store.child(root, "SpeedRate") {
            store.delete_class(root, id).unwrap();
        }
        store
            .set_attribute(root, "InertiaAccelerationY", Some("0.875"))
            .unwrap();
        let p = ShipParameters::load(&store, root).unwrap();
        p.apply_inertia(&mut state).unwrap();
        assert_eq!(snapshot(p, state, mounted, &rows[2]["calls"]), rows[2]);
    }
}
#[test]
fn defaults_literal_names_prefix_parsing_and_signed_dwords_follow_attribute_contract() {
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    assert_eq!(
        ShipParameters::load(&store, root).unwrap(),
        ShipParameters::default()
    );
    store
        .set_attribute(root, "sPeEdRaTe", Some(" 12.5 knots"))
        .unwrap();
    store.set_attribute(root, "Class", Some("-1")).unwrap();
    store
        .set_attribute(root, "Capacity", Some("2147483648"))
        .unwrap();
    store
        .set_attribute(root, "InertiaAcceleration.X", Some("10"))
        .unwrap();
    let p = ShipParameters::load(&store, root).unwrap();
    assert_eq!(p.speed_rate, 12.5);
    assert_eq!(p.class, -1);
    assert_eq!(p.capacity, i32::MIN);
    assert_eq!(p.inertia_accel, [0.0; 3]);
    let frozen = p;
    store.set_attribute(root, "SpeedRate", Some("9")).unwrap();
    assert_eq!(frozen.speed_rate, 12.5);
    assert_eq!(ShipParameters::load(&store, root).unwrap().speed_rate, 9.0);
    assert!(ShipParameters::load(&AttributeStore::default(), root).is_err());
    store.drop_root(root).unwrap();
    assert!(ShipParameters::load(&store, root).is_err());
}
#[test]
fn malformed_float_parameters_and_bounds_do_not_partially_configure_motion() {
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    for value in ["nan", "inf", "-inf"] {
        store.set_attribute(root, "Weight", Some(value)).unwrap();
        assert!(ShipParameters::load(&store, root).is_err());
    }
    let mut state = MotionState {
        position: [3.0, 7.0, -5.0],
        mass_inertia: 7.0,
        ..Default::default()
    };
    let saved = state;
    let mut placement = ShipPlacement {
        immersion: 0.75,
        waterline: 1.25,
    };
    let old = placement;
    let p = ShipParameters {
        inertia_accel: [f32::NAN, 1.0, 2.0],
        ..Default::default()
    };
    assert!(p.configure(&mut state, &mut placement, [1.0; 3]).is_err());
    assert_eq!(state, saved);
    assert_eq!(placement, old);
    assert!(
        ShipParameters::default()
            .configure(&mut state, &mut placement, [f32::INFINITY, 1.0, 2.0])
            .is_err()
    );
    assert_eq!(state, saved);
    assert_eq!(placement, old);
}
