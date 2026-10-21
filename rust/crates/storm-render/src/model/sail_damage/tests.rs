use super::*;
use storm_sim::RigState;
fn calm() -> SailWind {
    SailWind {
        speed: 0.0,
        direction: 0.0,
    }
}
#[test]
fn swept_shots_pierce_both_sail_shapes_and_holes_persist_and_reduce_absolute_drive() {
    let asset = ModelAsset::sail_demo().unwrap();
    let mut state = ShipState::default();
    let intact = asset.rig_layout_at(&state, 0.0);
    let hits = asset
        .sail_volley(&mut state, 3, calm(), 0.0, RigState::uniform(1.0))
        .unwrap();
    assert_eq!(hits.iter().filter(|h| h.new_damage).count(), 6);
    assert_eq!(
        state
            .sail_holes
            .values()
            .map(|m| m.count_ones())
            .sum::<u32>(),
        6
    );
    let damaged = asset.rig_layout_at(&state, 0.0);
    assert!(damaged.missing_weight > 0.0);
    assert!((intact.normal_weight * 0.75 - damaged.normal_weight).abs() < 1e-6);
    assert!((intact.full_only_weight * 0.7 - damaged.full_only_weight).abs() < 1e-6);
    assert_eq!(
        damaged.missing_weight,
        asset.rig_layout_at(&state, 30.0).missing_weight
    );
    let saved = state.sail_holes.clone();
    assert!(
        asset
            .sail_volley(&mut state, 12, calm(), 0.0, RigState::uniform(0.0))
            .unwrap()
            .is_empty()
    );
    assert_eq!(saved, state.sail_holes);
    asset
        .sail_volley(&mut state, 12, calm(), 0.0, RigState::uniform(1.0))
        .unwrap();
    assert_eq!(asset.rig_layout_at(&state, 0.0).normal_weight, 0.0);
    assert_eq!(asset.rig_layout_at(&state, 0.0).full_only_weight, 0.0);
    assert_eq!(
        intact.normal_weight + intact.full_only_weight,
        asset.rig_layout_at(&state, 0.0).missing_weight
    );
}
#[test]
fn original_coarse_traces_keep_colliding_with_damaged_sectors_and_reject_invalid_state() {
    let mut asset = ModelAsset::sail_demo().unwrap();
    asset.meshes.truncate(1);
    let patch = asset.meshes[0].sail.as_ref().unwrap();
    let wind = SailWind {
        speed: 16.0,
        direction: 0.0,
    };
    let time = 2.0;
    let vertices = patch.vertices(wind, time).unwrap();
    let [a, b, c] = patch.trace_cells()[5]
        .1
        .map(|i| Vec3::from_array(vertices[i as usize].position));
    let p = a * 0.2 + b * 0.3 + c * 0.5;
    let pose = ModelPose {
        translation: [20.0, 3.0, -11.0],
        yaw: 0.6,
        ..Default::default()
    };
    let a = pose.transform_point(p - Vec3::Z * 5.0).unwrap();
    let b = pose.transform_point(p + Vec3::Z * 5.0).unwrap();
    let world_wind = SailWind {
        speed: 16.0,
        direction: pose.yaw,
    };
    let mut state = ShipState::default();
    let hits = asset
        .trace_sail_shot(
            &mut state,
            a,
            b,
            world_wind,
            time,
            RigState::uniform(1.0),
            pose,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].new_damage);
    assert!((hits[0].fraction - 0.5).abs() < 1e-4);
    let mask = state.sail_holes[&0];
    let repeated = asset
        .trace_sail_shot(
            &mut state,
            a,
            b,
            world_wind,
            time,
            RigState::uniform(1.0),
            pose,
        )
        .unwrap();
    assert_eq!(repeated.len(), 1);
    assert!(!repeated[0].new_damage);
    assert_eq!(state.sail_holes[&0], mask);
    assert!(
        asset
            .trace_sail_shot(
                &mut state,
                Vec3::NAN,
                b,
                world_wind,
                time,
                RigState::uniform(1.0),
                pose
            )
            .is_err()
    );
    state.sail_holes.insert(50, 1);
    assert!(
        asset
            .trace_sail_shot(
                &mut state,
                a,
                b,
                world_wind,
                time,
                RigState::uniform(1.0),
                pose
            )
            .is_err()
    );
}
#[test]
fn penetrating_hits_are_sorted_and_broken_masts_stop_receiving_sail_damage() {
    let mut asset = ModelAsset::sail_demo().unwrap();
    let mut second = asset.meshes.remove(0);
    // Keep exactly overlapping cloth; stable tie order is draw order.
    second.ship_node = None;
    let first = Mesh::sail(second.sail.clone().unwrap(), 0).unwrap();
    asset.meshes = vec![first, second];
    let mut state = ShipState::default();
    let hits = asset
        .trace_sail_shot(
            &mut state,
            Vec3::new(-2.25, 4.0, -2.0),
            Vec3::new(-2.25, 4.0, 2.0),
            calm(),
            0.0,
            RigState::uniform(1.0),
            ModelPose::default(),
        )
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!((hits[0].mesh, hits[1].mesh), (0, 1));
    let asset = ModelAsset::ship_demo().unwrap();
    let mut state = ShipState::default();
    state.broken_masts.insert(2, 0.0);
    state.broken_masts.insert(1, 0.0);
    assert!(
        asset
            .sail_volley(&mut state, 12, calm(), 1.0, RigState::uniform(1.0))
            .unwrap()
            .is_empty()
    );
}
#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn gpu_cutouts_follow_flutter_and_furling_and_repair_restores_the_exact_frame() {
    let asset = ModelAsset::sail_demo().unwrap();
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let camera = OrbitCamera::new(&asset);
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let mut state = ShipState::default();
    asset
        .sail_volley(&mut state, 6, calm(), 0.0, RigState::uniform(1.0))
        .unwrap();
    let mut capture = |state, rig, time| {
        scene.set_ship_state(state).unwrap();
        scene.set_rig_state(rig).unwrap();
        crate::render_offscreen_with(&gpu, 321, 181, time, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                time,
                &camera,
                SailWind::default(),
            )
        })
        .unwrap()
    };
    let original = capture(ShipState::default(), RigState::uniform(1.0), 0.0);
    let damaged = capture(state.clone(), RigState::uniform(1.0), 0.0);
    assert!(
        original
            .rgba
            .iter()
            .zip(&damaged.rgba)
            .filter(|(a, b)| a != b)
            .count()
            > 100
    );
    capture(state.clone(), RigState::uniform(0.0), 5.0);
    capture(state.clone(), RigState::uniform(1.0), 8.0);
    assert_eq!(
        damaged.rgba,
        capture(state, RigState::uniform(1.0), 0.0).rgba
    );
    assert_eq!(
        original.rgba,
        capture(ShipState::default(), RigState::uniform(1.0), 0.0).rgba
    );
}

#[test]
fn damage_indices_uvs_and_coarse_hits_match_retained_cpp_reference() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("reference/cases.json")).unwrap();
    let patches = [
        SailPatch::new(SailShape::Quad([
            Vec3::ZERO,
            Vec3::X * 4.0,
            Vec3::new(4.0, 6.0, 0.0),
            Vec3::Y * 6.0,
        ]))
        .unwrap(),
        SailPatch::new(SailShape::Triangle([
            Vec3::ZERO,
            Vec3::X * 4.0,
            Vec3::Y * 6.0,
        ]))
        .unwrap(),
    ];
    let vertices = patches
        .iter()
        .map(|patch| {
            let mut v = patch.vertices(calm(), 0.0).unwrap();
            for p in &mut v {
                p.position[2] = 0.25 * (p.position[0] / 4.0) * (p.position[1] / 6.0);
            }
            v
        })
        .collect::<Vec<_>>();
    for (index, case) in data["cases"].as_array().unwrap().iter().enumerate() {
        let triangle = case["triangle"].as_u64().unwrap() as usize;
        let patch = &patches[triangle];
        match case["type"].as_str().unwrap() {
            "indices" => {
                let mask = case["mask"].as_u64().unwrap() as u16;
                let topology = patch.original_indices(mask);
                assert_eq!(
                    u64::from(topology.normal_count),
                    case["normal_count"].as_u64().unwrap(),
                    "case {index}"
                );
                let mut hash = 14695981039346656037u64;
                for &value in &topology.indices {
                    assert!((value as usize) < patch.vertex_count());
                    for b in (value as u16).to_le_bytes() {
                        hash ^= u64::from(b);
                        hash = hash.wrapping_mul(1099511628211);
                    }
                }
                assert_eq!(
                    format!("{hash:x}"),
                    case["hash"].as_str().unwrap(),
                    "case {index}"
                );
            }
            "uv" => {
                for (v, c) in vertices[triangle]
                    .iter()
                    .zip(case["vertices"].as_array().unwrap())
                {
                    let values = [
                        v.uv[0],
                        v.uv[1],
                        1.0 + v.uv[0] * 0.5,
                        v.uv[1] * 0.5 + if triangle == 1 { 0.5 } else { 0.0 },
                    ];
                    for (a, b) in values.into_iter().zip(c.as_array().unwrap()) {
                        assert!(
                            (f64::from(a) - b.as_f64().unwrap()).abs() < 1e-7,
                            "case {index}"
                        );
                    }
                }
            }
            "speed" => {
                let result = original_sail_speed(
                    case["holes"].as_i64().unwrap() as i32,
                    case["maximum"].as_i64().unwrap() as i32,
                    37.625,
                    case["dependence"].as_f64().unwrap() as f32,
                );
                assert_eq!(
                    result,
                    case["result"].as_f64().unwrap() as f32,
                    "case {index}"
                );
            }
            "trace" => {
                let vector = |key: &str| {
                    Vec3::new(
                        case[key][0].as_f64().unwrap() as f32,
                        case[key][1].as_f64().unwrap() as f32,
                        case[key][2].as_f64().unwrap() as f32,
                    )
                };
                let old = case["mask"].as_u64().unwrap() as u16;
                let hit =
                    patch.trace_original(&vertices[triangle], vector("from"), vector("to"), old);
                let (fraction, mask) = hit.map_or((2.0, old), |(t, _, _, m)| (t, m));
                assert_eq!(
                    u64::from(mask),
                    case["result_mask"].as_u64().unwrap(),
                    "case {index} {case}"
                );
                assert!(
                    (f64::from(fraction) - case["fraction"].as_f64().unwrap()).abs() < 1e-6,
                    "case {index} {case}; fraction {fraction}"
                );
            }
            _ => panic!("unknown original case"),
        }
    }
}

#[test]
fn original_dependence_settings_and_mask_restoration_are_bounded() {
    let settings = SailDamageSettings::from_ini(
        "[SAILS]\nfHoleDepend=0.5\nfTHoleFlexDepend=0.2\nfSHoleFlexDepend=0.2",
    )
    .unwrap();
    assert_eq!(settings.speed_depend, 0.5);
    assert_eq!(settings.triangle_flex, 0.1);
    assert_eq!(settings.quad_flex, 1.0 / 12.0);
    assert!(SailDamageSettings::from_ini("[SAILS]\nfHoleDepend=NaN").is_err());
    let mut asset = ModelAsset::sail_demo().unwrap();
    let mut state = ShipState::default();
    for index in 0..2 {
        asset.set_sail_holes(&mut state, index, u32::MAX).unwrap();
        let patch = asset.meshes[index].sail.as_mut().unwrap();
        assert_eq!(state.sail_holes[&index], (1 << patch.section_count()) - 1);
        patch.damage_settings = settings;
        assert!((patch.intact_fraction(state.sail_holes[&index]) - 0.5).abs() < 1e-7);
    }
    assert!(asset.set_sail_holes(&mut state, 2, 1).is_err());
}
