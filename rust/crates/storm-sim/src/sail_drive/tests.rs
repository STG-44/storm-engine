use super::*;
use serde_json::{Value as Json, json};
fn snapshot(g: &Group, output: f32, calls: &[RollRequest]) -> Json {
    json!({"group":[g.speed.to_bits(),g.maximum.to_bits(),g.reference.to_bits(),g.speed_multiplier.to_bits(),g.current_set as u32],"output":output.to_bits(),"calls":calls.iter().map(|c|[c.sail as u32,u32::from(c.roll_up)]).collect::<Vec<_>>()})
}
#[test]
fn sail_wind_capacity_and_speed_requests_match_complete_native_snapshots() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_drive/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 96);
    for id in 0..96 {
        let c = &data["cases"][id];
        let id = id as i32;
        let wind = [
            (id % 5) as f32 * 0.5 - 1.0,
            (id % 3) as f32 * 0.25 - 0.25,
            (id % 7) as f32 * 0.5 - 1.5,
        ];
        let mut sails = Vec::new();
        for j in 0..3 {
            let kind = [
                Kind::Triangle,
                Kind::Square,
                Kind::Trapezoid,
                Kind::Gerbed,
                Kind::Rolling,
                Kind::Special,
            ][((id + j) % 6) as usize];
            let points = [
                [0.0; 3],
                [2.0 + id as f32 * 0.125, 0.0, 0.25],
                [2.5, 3.0 + j as f32, 0.5],
                [0.0, 4.0, 0.0],
            ];
            let maximum = capacity(points, kind == Kind::Triangle).unwrap();
            let s = Sail {
                kind,
                maximum,
                holes: (id + j) % 16,
                hole_maximum: if kind == Kind::Triangle { 10 } else { 12 },
                hole_dependence: (id % 4) as f32 * 0.5,
                axes: [
                    [1.0, j as f32 * 0.125, 0.0],
                    [0.0, 1.0, 0.25],
                    [(id % 3) as f32 * 0.125, 0.0, 1.0],
                ],
                rolling: (id + j) % 7 == 0,
                rolling_delta: ((id + j) % 4 == 0).then_some((id % 5) as f32 * 0.25),
                roll_type: 1 + j % 2,
            };
            let o = s.evaluate(wind, Coefficients::default()).unwrap();
            assert_eq!(
                json!({"capacity":maximum.to_bits(),"wind":o.wind.map(f32::to_bits),"current":o.current.to_bits(),"maximum":o.maximum.to_bits()}),
                c["sails"][j as usize],
                "profile {id} sail {j}"
            );
            sails.push(s);
        }
        let mut reference = 0.0;
        for s in &sails {
            reference += s.maximum;
        }
        if id % 13 == 0 {
            reference = 0.0005;
        }
        let mut groups = vec![Group {
            ship: 100,
            deleted: false,
            current_set: id % 3,
            speed: 0.0,
            maximum: 0.0,
            reference,
            speed_multiplier: 0.75,
            sails,
        }];
        groups[0].evaluate(wind, Coefficients::default()).unwrap();
        assert_eq!(snapshot(&groups[0], 7.5, &[]), c["snapshots"][0]);
        groups[0].deleted = id % 11 == 0;
        let requests = [
            0.0,
            0.3,
            f32::from_bits(0.3_f32.to_bits() + 1),
            0.7,
            f32::from_bits(0.7_f32.to_bits() + 1),
            1.0,
            -0.5,
            1.5,
        ];
        for step in 0..3 {
            let mut output = 7.5;
            let destination = (id % 10 != 0).then_some(&mut output);
            let calls = request_speed(
                &mut groups,
                if id % 9 == 0 { 999 } else { 100 },
                destination,
                if step == 0 {
                    requests[(id % 8) as usize]
                } else {
                    (step - 1) as f32
                },
                (id % 3) as f32 * 0.125,
            )
            .unwrap();
            assert_eq!(
                snapshot(&groups[0], output, &calls),
                c["snapshots"][step as usize + 1],
                "profile {id} step {step}"
            );
        }
    }
}
#[test]
fn independent_groups_null_destinations_and_invalid_inputs_preserve_state() {
    let sail = Sail {
        kind: Kind::Square,
        maximum: 2.0,
        holes: 0,
        hole_maximum: 12,
        hole_dependence: 1.0,
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        rolling: false,
        rolling_delta: None,
        roll_type: 2,
    };
    let g = Group {
        ship: 100,
        deleted: false,
        current_set: 2,
        speed: 0.5,
        maximum: 1.0,
        reference: 2.0,
        speed_multiplier: 0.75,
        sails: vec![sail],
    };
    let mut groups = vec![
        g.clone(),
        Group {
            ship: 200,
            ..g.clone()
        },
        g,
    ];
    let old = snapshot(&groups[1], 7.5, &[]);
    let mut output = 7.5;
    let calls = request_speed(&mut groups, 100, Some(&mut output), 0.0, 0.25).unwrap();
    assert_eq!(
        calls,
        vec![RollRequest {
            group: 0,
            sail: 0,
            roll_up: true
        }]
    );
    assert_eq!(output, 0.5);
    assert_eq!(snapshot(&groups[1], 7.5, &[]), old);
    assert_eq!(groups[2].current_set, 2);
    let before = snapshot(&groups[0], output, &[]);
    request_speed(&mut groups, 100, None, f32::NAN, f32::NAN).unwrap();
    assert_eq!(snapshot(&groups[0], output, &[]), before);
    assert!(request_speed(&mut groups, 100, Some(&mut output), f32::NAN, 0.0).is_err());
    assert_eq!(snapshot(&groups[0], output, &[]), before);
    let mut s = groups[0].sails[0].clone();
    s.axes[1][2] = f32::INFINITY;
    assert!(
        s.evaluate([0.0, 0.0, 1.0], Coefficients::default())
            .is_err()
    );
    let mut points = [[0.0; 3]; 4];
    points[3][0] = f32::NAN;
    assert!(capacity(points, false).is_err());
    groups[0].deleted = true;
    assert!(
        groups[0]
            .evaluate([f32::NAN; 3], Coefficients::default())
            .unwrap()
            .is_empty()
    );
}
