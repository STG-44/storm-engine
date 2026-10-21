use super::*;
use storm_formats::particles::{ColorPoint, Field, GraphPoint, Text};
fn field(name: &str, value: Value) -> Field {
    Field {
        name: Text(name.as_bytes().to_vec()),
        value,
    }
}
fn constant(value: f32) -> Graph {
    Graph {
        negative: true,
        relative: true,
        max: vec![
            GraphPoint { time: 0.0, value },
            GraphPoint { time: 100.0, value },
        ],
        min: vec![
            GraphPoint { time: 0.0, value },
            GraphPoint { time: 100.0, value },
        ],
    }
}
pub fn fixture() -> ParticleStyle {
    let emitter = Fields(vec![
        field("Life time", Value::Float(1.0)),
        field("Position", Value::Position([1.0, 2.0, 3.0])),
        field("Looping", Value::Bool(true)),
        field("Physics Dir X", Value::Graph(constant(0.0))),
        field("Physics Dir Y", Value::Graph(constant(0.0))),
        field("Physics Dir Z", Value::Graph(constant(0.0))),
    ]);
    let mut particles = Fields(vec![
        field("Max Particles count", Value::Float(8.0)),
        field("Texture", Value::Uv(vec![[0.25, 0.5, 0.5, 0.75]])),
        field(
            "Color",
            Value::Color(ColorGraph(vec![
                ColorPoint {
                    time: 0.0,
                    max: [0.25, 0.5, 1.0, 0.1],
                    min: [0.25, 0.5, 1.0, 0.1],
                },
                ColorPoint {
                    time: 1.0,
                    max: [0.25, 0.5, 1.0, 0.1],
                    min: [0.25, 0.5, 1.0, 0.1],
                },
            ])),
        ),
    ]);
    for (name, value) in [
        ("Life time", 2.0),
        ("Emission rate", 4.0),
        ("Size", 0.5),
        ("Transparency", 25.0),
        ("Animation frame", 0.0),
        ("Track X", 0.5),
        ("Track Y", 1.0),
        ("Track Z", 2.0),
        ("Mass", 1.0),
        ("Spin", 0.0),
        ("Velocity", 0.0),
        ("Friction", 0.0),
        ("Spin Friction", 0.0),
        ("ForceField_gravity", 0.0),
        ("Physic blend", 0.0),
    ] {
        particles.0.push(field(name, Value::Graph(constant(value))));
    }
    ParticleStyle::read("generated.xps".into(), &emitter, &particles).unwrap()
}
#[test]
fn named_profile_selects_its_own_geometry_across_fire_channels_and_stops_its_tail() {
    use super::super::*;
    let asset = ModelAsset::ship_demo().unwrap();
    let mut style = fixture();
    style.preview = false;
    let patch = ParticlePatch {
        points: std::sync::Arc::new(asset.ship.fires.clone()),
        smoke: false,
        per_point: style.count,
        lifetime: 2.0,
        style: Some(std::sync::Arc::new(style)),
    };
    let dynamic = DynamicMesh::Particles(patch.clone());
    let mut state = ShipState {
        fire_started: Some(0.0),
        ..Default::default()
    };
    assert!(
        !dynamic.visible(&state, 1.0),
        "an additional cached profile waits for a native message"
    );
    state.native_fire = Some(vec![FireEmission {
        point: 0,
        smoke: true,
        started: 0.0,
        stopped: Some(1.0),
        profile: Some("generated.xps".into()),
    }]);
    assert!(dynamic.visible(&state, 1.0));
    assert!(patch.emission(&state, 0).is_some());
    let mut vertices = vec![bytemuck::Zeroable::zeroed(); dynamic.vertex_count()];
    dynamic
        .deform(
            &mut vertices,
            1.0,
            SailWind::default(),
            glam::Vec3::splat(10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(vertices.iter().any(|v| v.color[3] > 0.0));
    assert!(!dynamic.visible(&state, 3.0));
    state.native_fire.as_mut().unwrap()[0].profile = Some("other.xps".into());
    assert!(!dynamic.visible(&state, 1.0));
    dynamic
        .deform(
            &mut vertices,
            1.0,
            SailWind::default(),
            glam::Vec3::splat(10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(vertices.iter().all(|v| v.color[3] == 0.0));
}
#[test]
fn xps_appearance_uses_curve_size_inverse_transparency_color_uv_and_reflected_track() {
    let style = fixture();
    let sample = style.sample(1.0, 0.0, 0).unwrap();
    assert_eq!(sample.radius, 0.5);
    assert_eq!(sample.color, [0.25, 0.5, 1.0, 0.75]);
    assert_eq!(sample.uv, [0.25, 0.5, 0.5, 0.75]);
    assert_eq!(sample.offset, Vec3::new(1.5, 3.0, -5.0));
    assert!(style.sample(0.0, 1.0, 0).is_none());
    assert!(style.sample(0.1, 0.0, 0).is_none());
    let mut nonloop = style.clone();
    nonloop.looping = false;
    assert!(nonloop.sample(5.0, 0.0, 0).is_none());
}
#[test]
fn invalid_or_incomplete_styles_fail_and_nonfinite_samples_cannot_reach_vertices() {
    assert!(ParticleStyle::read("broken".into(), &Fields(vec![]), &Fields(vec![])).is_err());
    let mut style = fixture();
    style.size = constant(f32::NAN);
    assert!(style.sample(1.0, 0.0, 0).is_none());
    let mut empty = fixture();
    empty.frames.clear();
    assert!(empty.sample(1.0, 0.0, 0).is_none());
    assert!(fixture().sample(f32::NAN, 0.0, 0).is_none());
    assert!(fixture().sample(1.0, 0.0, usize::MAX).is_none());
    let mut style = fixture();
    style.rate = constant(0.0);
    assert!(style.sample(1.0, 0.0, 0).is_none());
}
#[test]
fn xps_vertices_stay_inside_atlas_frames_and_native_stop_does_not_restart_emission() {
    use crate::model::*;
    use bytemuck::Zeroable;
    use std::sync::Arc;
    let asset = ModelAsset::ship_demo().unwrap();
    let style = Arc::new(fixture());
    let patch = ParticlePatch {
        points: Arc::new(asset.ship.fires.clone()),
        smoke: false,
        per_point: style.count,
        lifetime: style.lifetime.maximum(),
        style: Some(style),
    };
    let dynamic = DynamicMesh::Particles(patch);
    let mut out = vec![ModelVertex::zeroed(); dynamic.vertex_count()];
    let mut state = ShipState {
        native_fire: Some(vec![FireEmission {
            point: 0,
            smoke: false,
            started: 0.0,
            stopped: Some(1.0),
            profile: None,
        }]),
        ..Default::default()
    };
    dynamic
        .deform(
            &mut out,
            1.25,
            SailWind::default(),
            Vec3::splat(10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(out.iter().any(|v| v.color[3] > 0.0));
    assert!(
        out.iter()
            .filter(|v| v.color[3] > 0.0)
            .all(|v| (0.25..=0.5).contains(&v.uv[0]) && (0.5..=0.75).contains(&v.uv[1]))
    );
    dynamic
        .deform(
            &mut out,
            3.25,
            SailWind::default(),
            Vec3::splat(10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(out.iter().all(|v| v.color[3] == 0.0));
    state.native_fire = Some(vec![]);
    state.fire_started = Some(0.0);
    dynamic
        .deform(
            &mut out,
            1.0,
            SailWind::default(),
            Vec3::splat(10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(out.iter().all(|v| v.color[3] == 0.0));
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn xps_curve_billboards_sample_the_selected_atlas_cell_and_clear_on_the_gpu() {
    use crate::{Gpu, model::*};
    use bytemuck::Zeroable;
    use std::sync::Arc;
    use storm_formats::{
        TxHeader,
        texture::{Mip, Texture},
    };
    let mut asset = ModelAsset::ship_demo().unwrap();
    asset
        .meshes
        .retain(|m| matches!(&m.dynamic,Some(DynamicMesh::Particles(p)) if !p.smoke));
    let mut rgba = vec![];
    for y in 0..16 {
        for x in 0..16 {
            rgba.extend(if (4..8).contains(&x) && (8..12).contains(&y) {
                [0, 255, 0, 255]
            } else {
                [255, 0, 0, 255]
            });
        }
    }
    let texture = asset.textures.len();
    asset.textures.push(Texture {
        header: TxHeader {
            flags: 0,
            width: 16,
            height: 16,
            mips: 1,
            format: 21,
            mip0_bytes: 1024,
        },
        mips: vec![Mip {
            width: 16,
            height: 16,
            rgba,
        }],
    });
    let mut moving_style = fixture();
    moving_style.motion.velocity = constant(0.75);
    moving_style.motion.spin = constant(45.0);
    let style = Arc::new(moving_style);
    let patch = ParticlePatch {
        points: Arc::new(asset.ship.fires.clone()),
        smoke: false,
        per_point: style.count,
        lifetime: style.lifetime.maximum(),
        style: Some(style),
    };
    let mesh = &mut asset.meshes[0];
    mesh.vertices = vec![ModelVertex::zeroed(); patch.per_point * 4];
    mesh.indices = patch.indices();
    mesh.dynamic = Some(DynamicMesh::Particles(patch));
    mesh.texture = texture;
    asset.ship.particle_profiles = Some(std::collections::BTreeSet::from([
        "generated.xps".into(),
        "other.xps".into(),
    ]));
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    let p = storm_sim::particle_motion::Transform::from_euler([0.0, 1.0, 0.0], [0.0; 3])
        .unwrap()
        .point([1.5, 3.0, 5.0]);
    camera.target = asset.ship.fires[0].position + Vec3::new(p[0], p[1], -p[2]);
    camera.distance = 10.0;
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let history = Arc::new(storm_sim::pose_history::PoseHistory::default());
    history.record(ModelPose::default().sample(0.0)).unwrap();
    history.record(ModelPose::default().sample(2.0)).unwrap();
    let mut capture = |mut state: ShipState, pose| {
        state.particle_history = Some(history.clone());
        scene.set_pose(pose).unwrap();
        scene.set_ship_state(state).unwrap();
        crate::render_offscreen_with(&gpu, 321, 181, 2.0, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                2.0,
                &camera,
                SailWind::default(),
            )
        })
        .unwrap()
    };
    let unlit = capture(ShipState::default(), ModelPose::default());
    let active = capture(
        ShipState {
            native_fire: Some(vec![FireEmission {
                point: 0,
                smoke: false,
                started: 0.0,
                stopped: None,
                profile: None,
            }]),
            ..Default::default()
        },
        ModelPose::default(),
    );
    let green = |frame: &crate::Frame| {
        frame
            .rgba
            .chunks_exact(4)
            .filter(|p| {
                u16::from(p[1]) > u16::from(p[0]) * 2 && u16::from(p[1]) > u16::from(p[2]) * 2
            })
            .count()
    };
    assert!(green(&active) > green(&unlit) + 5);
    let repeated = capture(
        ShipState {
            native_fire: Some(vec![FireEmission {
                point: 0,
                smoke: false,
                started: 0.0,
                stopped: None,
                profile: None,
            }]),
            ..Default::default()
        },
        ModelPose::default(),
    );
    assert_eq!(active.rgba, repeated.rgba);
    let named = |key: &str| ShipState {
        native_fire: Some(vec![FireEmission {
            point: 0,
            smoke: true,
            started: 0.0,
            stopped: None,
            profile: Some(key.into()),
        }]),
        ..Default::default()
    };
    assert_eq!(
        active.rgba,
        capture(named("generated.xps"), ModelPose::default()).rgba
    );
    assert_eq!(
        unlit.rgba,
        capture(named("other.xps"), ModelPose::default()).rgba
    );
    let relocated = capture(
        ShipState {
            native_fire: Some(vec![FireEmission {
                point: 0,
                smoke: false,
                started: 0.0,
                stopped: None,
                profile: None,
            }]),
            ..Default::default()
        },
        ModelPose {
            translation: [20.0, 3.0, 0.0],
            yaw: 1.2,
            pitch: 0.2,
            roll: 0.1,
        },
    );
    assert_eq!(
        active.rgba, relocated.rgba,
        "existing world particles must not inherit the current ship pose"
    );
    assert_eq!(
        unlit.rgba,
        capture(
            ShipState {
                native_fire: Some(vec![]),
                ..Default::default()
            },
            ModelPose::default()
        )
        .rgba
    );
}

fn sample_signature(samples: Vec<Option<StyleSample>>) -> Vec<(u64, [u32; 4], [u32; 3], u32)> {
    samples
        .into_iter()
        .flatten()
        .map(|s| {
            (
                s.serial,
                s.color.map(f32::to_bits),
                s.offset.to_array().map(f32::to_bits),
                s.angle.to_bits(),
            )
        })
        .collect()
}
#[test]
fn world_particles_keep_birth_positions_after_the_ship_moves_and_emission_pauses() {
    use crate::model::{ModelPose, WorldEmitter};
    use std::sync::Arc;
    let mut style = fixture();
    style.emitter_position = Vec3::ZERO;
    style.track = std::array::from_fn(|_| constant(0.0));
    style.count = 100;
    style.rate = constant(60.0);
    style.lifetime = constant(3.0);
    let history = Arc::new(storm_sim::pose_history::PoseHistory::default());
    history.record(ModelPose::default().sample(0.0)).unwrap();
    history
        .record(
            ModelPose {
                translation: [10.0, 0.0, 0.0],
                ..Default::default()
            }
            .sample(1.0),
        )
        .unwrap();
    let world = WorldEmitter {
        history: history.clone(),
        point: [0.0; 3],
    };
    let (first, births, _) = style
        .world_samples(0.5, 0.0, Some(0.5), 0, world.clone())
        .unwrap();
    let (_, later_births, _) = style
        .world_samples(0.75, 0.0, Some(0.5), 0, world.clone())
        .unwrap();
    assert_eq!(births.created, later_births.created);
    history
        .record(
            ModelPose {
                translation: [100.0, 4.0, 0.0],
                yaw: 1.5,
                ..Default::default()
            }
            .sample(2.0),
        )
        .unwrap();
    let (later, _, _) = style
        .world_samples(1.0, 0.0, Some(0.5), 0, world.clone())
        .unwrap();
    let coords = |v: Vec<Option<StyleSample>>| {
        v.into_iter()
            .flatten()
            .map(|s| (s.serial, s.offset.to_array().map(f32::to_bits)))
            .collect::<Vec<_>>()
    };
    assert_eq!(coords(first), coords(later.clone()));
    assert!(later.iter().flatten().all(|s| s.offset.x < 5.0));
    assert_eq!(
        sample_signature(later),
        sample_signature(
            style
                .clone()
                .world_samples(1.0, 0.0, Some(0.5), 0, world.clone())
                .unwrap()
                .0
        )
    );
    style
        .world_samples(0.1, 0.0, Some(0.5), 0, world.clone())
        .unwrap();
    assert_eq!(
        sample_signature(
            style
                .world_samples(1.0, 0.0, Some(0.5), 0, world)
                .unwrap()
                .0
        ),
        sample_signature(
            style
                .clone()
                .world_samples(
                    1.0,
                    0.0,
                    Some(0.5),
                    0,
                    WorldEmitter {
                        history,
                        point: [0.0; 3]
                    }
                )
                .unwrap()
                .0
        )
    );
}
#[test]
fn world_emitter_tracks_rotated_locators_but_retains_native_fixed_fire_angles() {
    use crate::model::{ModelPose, WorldEmitter};
    use std::sync::Arc;
    let history = Arc::new(storm_sim::pose_history::PoseHistory::default());
    history.record(ModelPose::default().sample(0.0)).unwrap();
    history
        .record(
            ModelPose {
                yaw: std::f32::consts::FRAC_PI_2,
                translation: [4.0, 2.0, 0.0],
                ..Default::default()
            }
            .sample(1.0),
        )
        .unwrap();
    let world = WorldEmitter {
        history,
        point: [2.0, 0.0, 0.0],
    };
    let a = world.at(0.0).unwrap();
    let b = world.at(1.0).unwrap();
    assert!(
        (b.0[3][0] - 4.0).abs() < 1e-5
            && (b.0[3][1] - 2.0).abs() < 1e-5
            && (b.0[3][2] - 2.0).abs() < 1e-5
    );
    assert_eq!(a.0[..3], b.0[..3]);
    let half = world.at(0.5).unwrap();
    assert!((half.0[3][0] - 3.0).abs() < 1e-5);
    assert!(world.at(f64::NAN).is_err());
    assert!(fixture().world_samples(2.0, 0.0, None, 0, world).is_err());
}

#[test]
fn profile_motion_survives_emission_pause_and_replays_after_configuration_changes() {
    let mut style = fixture();
    style.motion.velocity = constant(2.0);
    style.motion.mass = constant(-1.0);
    style.motion.gravity = constant(100.0);
    style.motion.spin = constant(90.0);
    let before = style.birth_stats(1.0, 0.0, Some(1.0), 0).unwrap();
    let initial = style.samples(1.0, 0.0, Some(1.0), 0).unwrap();
    let later = style.samples(1.25, 0.0, Some(1.0), 0).unwrap();
    assert_eq!(
        before.created,
        style.birth_stats(1.25, 0.0, Some(1.0), 0).unwrap().created
    );
    let a = initial[0].as_ref().unwrap();
    let b = later[0].as_ref().unwrap();
    assert_eq!(a.serial, b.serial);
    assert!(b.offset.y > a.offset.y && b.angle > a.angle);
    let stats = style.motion_stats(1.25, 0.0, Some(1.0), 0).unwrap();
    assert_eq!(stats.living, stats.moving);
    assert_eq!(stats.living, stats.spinning);
    assert!(stats.maximum_speed > 2.0);
    assert_eq!(
        sample_signature(later),
        sample_signature(style.clone().samples(1.25, 0.0, Some(1.0), 0).unwrap())
    );
    style.motion.velocity = constant(0.0);
    style.motion.gravity = constant(0.0);
    style.motion.spin = constant(0.0);
    assert_eq!(
        style.motion_stats(1.25, 0.0, Some(1.0), 0).unwrap().moving,
        0
    );
    assert_eq!(
        sample_signature(style.samples(1.25, 0.0, Some(1.0), 0).unwrap()),
        sample_signature(fixture().samples(1.25, 0.0, Some(1.0), 0).unwrap())
    );
    style.motion.drag = constant(f32::NAN);
    assert!(style.samples(1.0, 0.0, None, 0).is_err());
}

#[test]
fn billboard_spin_rotates_the_quad_without_changing_atlas_coordinates() {
    use crate::model::*;
    use bytemuck::Zeroable;
    use std::sync::Arc;
    let mut style = fixture();
    style.motion.spin = constant(90.0);
    style.rate = constant(4.0);
    let asset = ModelAsset::ship_demo().unwrap();
    let sample = style.samples(1.0, 0.0, None, 0).unwrap()[0]
        .clone()
        .unwrap();
    let dynamic = DynamicMesh::Particles(ParticlePatch {
        points: Arc::new(asset.ship.fires.clone()),
        smoke: false,
        per_point: style.count,
        lifetime: 2.0,
        style: Some(Arc::new(style)),
    });
    let mut out = vec![ModelVertex::zeroed(); dynamic.vertex_count()];
    dynamic
        .deform(
            &mut out,
            1.0,
            SailWind::default(),
            Vec3::splat(10.0),
            &asset.ship,
            &ShipState {
                fire_started: Some(0.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(sample.angle > 0.0);
    let mut plain = fixture();
    plain.rate = constant(4.0);
    let dynamic_plain = DynamicMesh::Particles(ParticlePatch {
        points: Arc::new(asset.ship.fires.clone()),
        smoke: false,
        per_point: plain.count,
        lifetime: 2.0,
        style: Some(Arc::new(plain)),
    });
    let mut unspun = vec![ModelVertex::zeroed(); out.len()];
    dynamic_plain
        .deform(
            &mut unspun,
            1.0,
            SailWind::default(),
            Vec3::splat(10.0),
            &asset.ship,
            &ShipState {
                fire_started: Some(0.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        out.iter()
            .zip(&unspun)
            .any(|(a, b)| a.position != b.position)
    );
    assert!(
        out.iter()
            .zip(&unspun)
            .all(|(a, b)| a.uv == b.uv && a.color == b.color)
    );
}
#[test]
fn birth_replay_is_independent_of_presentation_cadence_repeated_draws_and_rewinds() {
    let direct = fixture();
    let stepped = fixture();
    let expected = sample_signature(direct.samples(1.013, 0.0, None, 0).unwrap());
    assert_eq!(
        direct.birth_stats(1.0, 0.0, None, 0).unwrap().created,
        direct.birth_stats(1.013, 0.0, None, 0).unwrap().created
    );
    for time in [0.01, 0.2, 0.55, 1.0, 1.013] {
        stepped.samples(time, 0.0, None, 0).unwrap();
    }
    assert_eq!(
        sample_signature(stepped.samples(1.013, 0.0, None, 0).unwrap()),
        expected
    );
    assert_eq!(
        direct.birth_stats(1.013, 0.0, None, 0).unwrap(),
        stepped.birth_stats(1.013, 0.0, None, 0).unwrap()
    );
    stepped.samples(0.1, 0.0, None, 0).unwrap();
    assert_eq!(
        sample_signature(stepped.samples(1.013, 0.0, None, 0).unwrap()),
        expected
    );
    assert_eq!(
        sample_signature(stepped.samples(1.013, 0.0, None, 0).unwrap()),
        expected
    );
}
#[test]
fn paused_emitters_hold_the_native_clock_while_live_particles_expire_and_capacity_recovers() {
    let mut style = fixture();
    style.count = 2;
    style.rate = constant(64.0);
    style.lifetime = constant(0.25);
    let before = style.birth_stats(1.0, 0.0, Some(1.0), 0).unwrap();
    assert!(before.created > 2 && before.living <= 2);
    let held = style.birth_stats(1.1, 0.0, Some(1.0), 0).unwrap();
    assert_eq!(before.created, held.created);
    assert_eq!(before.clock_bits, held.clock_bits);
    assert_eq!(before.remainder_bits, held.remainder_bits);
    let expired = style.birth_stats(1.5, 0.0, Some(1.0), 0).unwrap();
    assert_eq!(expired.created, before.created);
    assert_eq!(expired.living, 0);
    assert_eq!(expired.clock_bits, before.clock_bits);
    let restart = style.birth_stats(2.0, 1.9, None, 0).unwrap();
    assert!(restart.created > 0 && restart.created < before.created);
}
#[test]
fn independent_locators_and_changed_stop_boundaries_replay_without_ghost_births() {
    let style = fixture();
    let a = sample_signature(style.samples(1.5, 0.0, None, 0).unwrap());
    style.samples(0.8, 0.0, None, 1).unwrap();
    assert_eq!(
        sample_signature(style.samples(1.5, 0.0, None, 0).unwrap()),
        a
    );
    let rewound_stop = sample_signature(style.samples(1.5, 0.0, Some(0.75), 0).unwrap());
    assert_eq!(
        rewound_stop,
        sample_signature(fixture().samples(1.5, 0.0, Some(0.75), 0).unwrap())
    );
    assert!(style.samples(1.0, 0.0, Some(-1.0), 0).is_err());
    assert!(style.samples(7201.0, 0.0, None, 0).is_err());
    assert!(style.samples(1.0, 0.0, None, usize::MAX).is_err());
}
