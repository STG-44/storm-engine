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
    ] {
        particles.0.push(field(name, Value::Graph(constant(value))));
    }
    ParticleStyle::read("generated.xps".into(), &emitter, &particles).unwrap()
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
    let style = Arc::new(fixture());
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
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    camera.target = asset.ship.fires[0].position + Vec3::new(1.5, 3.0, -5.0);
    camera.distance = 10.0;
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let mut capture = |state| {
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
    let unlit = capture(ShipState::default());
    let active = capture(ShipState {
        native_fire: Some(vec![FireEmission {
            point: 0,
            smoke: false,
            started: 0.0,
            stopped: None,
        }]),
        ..Default::default()
    });
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
    let repeated = capture(ShipState {
        native_fire: Some(vec![FireEmission {
            point: 0,
            smoke: false,
            started: 0.0,
            stopped: None,
        }]),
        ..Default::default()
    });
    assert_eq!(active.rgba, repeated.rgba);
    assert_eq!(
        unlit.rgba,
        capture(ShipState {
            native_fire: Some(vec![]),
            ..Default::default()
        })
        .rgba
    );
}

fn sample_signature(samples: Vec<Option<StyleSample>>) -> Vec<(u64, [u32; 4], [u32; 3])> {
    samples
        .into_iter()
        .flatten()
        .map(|s| {
            (
                s.serial,
                s.color.map(f32::to_bits),
                s.offset.to_array().map(f32::to_bits),
            )
        })
        .collect()
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
