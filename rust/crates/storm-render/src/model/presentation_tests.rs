use super::*;
use bytemuck::Zeroable;
use std::sync::Arc;
use storm_formats::waypoints::{DeckGraph, DeckPoint};

#[test]
fn deck_patrols_follow_links_avoid_mast_climbs_and_repeat_at_constant_speed() {
    let graph = DeckGraph {
        missing_links: 0,
        points: vec![
            DeckPoint {
                position: [0.0, 1.0, 0.0],
                kind: 0,
            },
            DeckPoint {
                position: [2.0, 1.0, 0.0],
                kind: 10,
            },
            DeckPoint {
                position: [2.0, 1.0, 3.0],
                kind: 0,
            },
            DeckPoint {
                position: [0.0, 15.0, 0.0],
                kind: 5,
            },
        ],
        links: vec![[0, 1], [1, 2], [0, 3]],
    };
    for id in 0..5 {
        let route = Patrol::from_graph(&graph, id).unwrap();
        assert!(route.points.iter().all(|p| p.y == 1.0));
        for segment in route.points.windows(2) {
            assert!(graph.links.iter().any(|&[a, b]| {
                let (a, b) = (
                    Vec3::from_array(graph.points[a].position),
                    Vec3::from_array(graph.points[b].position),
                );
                (segment[0] == a && segment[1] == b) || (segment[0] == b && segment[1] == a)
            }));
        }
        assert!(
            route
                .sample(0.0)
                .0
                .distance(route.sample(route.length / 1.3).0)
                < 1e-5
        );
        assert!((route.sample(0.1).0.distance(route.sample(0.0).0) - 0.13).abs() < 1e-5);
    }
}

#[test]
fn crew_skinning_animates_independently_of_patrol_and_rejects_bad_bone_indices() {
    let asset = ModelAsset::ship_demo().unwrap();
    let c = asset
        .meshes
        .iter()
        .find_map(|m| match &m.dynamic {
            Some(DynamicMesh::Crew(c)) => Some(c.clone()),
            _ => None,
        })
        .unwrap();
    let graph = DeckGraph {
        missing_links: 0,
        points: vec![DeckPoint {
            position: [0.0, 1.0, 0.0],
            kind: 0,
        }],
        links: vec![],
    };
    let c = CrewPatch {
        route: Arc::new(Patrol::from_graph(&graph, 0).unwrap()),
        phase: 0.0,
        ..c
    };
    let mut first = vec![bytemuck::Zeroable::zeroed(); c.source.len()];
    let mut later = first.clone();
    c.deform(&mut first, 0.0).unwrap();
    c.deform(&mut later, 0.2).unwrap();
    assert!(
        first
            .iter()
            .zip(&later)
            .any(|(a, b)| a.position != b.position)
    );
    assert!(
        later
            .iter()
            .all(|v| Vec3::from_array(v.position).is_finite()
                && (Vec3::from_array(v.normal).length() - 1.0).abs() < 1e-5)
    );
    let mut restored = later.clone();
    c.deform(&mut restored, 0.0).unwrap();
    assert!(
        first
            .iter()
            .zip(&restored)
            .all(|(a, b)| a.position == b.position)
    );
    let mut source = (*c.source).clone();
    source[0].skin.as_mut().unwrap().bones = [255, 255];
    assert!(
        CrewPatch {
            source: Arc::new(source),
            ..c
        }
        .deform(&mut later, 0.2)
        .is_err()
    );
}

#[test]
fn mast_number_groups_and_hierarchy_damage_preserve_surviving_rig_area() {
    for id in [2, 201, 202, 299] {
        assert!(mast_falls_with(2, id));
    }
    for id in [1, 3, 200, 300] {
        assert!(!mast_falls_with(2, id));
    }
    assert!(mast_falls_with(201, 202));
    assert!(!mast_falls_with(201, 200));
    assert!(mast_falls_with(u32::MAX, u32::MAX));
    let asset = ModelAsset::ship_demo().unwrap();
    let original = asset.rig_layout();
    let mut state = ShipState::default();
    state.broken_masts.insert(2, 1.0);
    state.validate(&asset.ship).unwrap();
    assert!(asset.ship.fallen_mast("rey_b2", &state).is_some());
    assert!(asset.ship.fallen_mast("mast201", &state).is_some());
    assert!(asset.ship.fallen_mast("mast1", &state).is_none());
    let before = asset.rig_layout_at(&state, 0.0);
    let after = asset.rig_layout_at(&state, 2.0);
    assert_eq!(before.missing_weight, 0.0);
    assert!(after.missing_weight > 0.0);
    assert!(
        (after.normal_weight + after.full_only_weight + after.missing_weight
            - original.normal_weight
            - original.full_only_weight)
            .abs()
            < 1e-8
    );
    assert!(storm_sim::RigState::uniform(1.0).fraction(after) < 1.0);
    state.broken_hulls.insert("shatter1".into(), 1.0);
    assert!(!asset.ship.hidden("shatter1", &state, 0.0));
    assert!(asset.ship.hidden("shatter1", &state, 2.0));
    assert!(!asset.ship.hidden("fixture", &state, 2.0));
    state.broken_masts.insert(999, 0.0);
    assert!(state.validate(&asset.ship).is_err());
}

#[test]
fn fire_billboards_face_the_camera_have_soft_alpha_and_follow_visibility_state() {
    let asset = ModelAsset::ship_demo().unwrap();
    let mesh = asset
        .meshes
        .iter()
        .find(|m| matches!(m.dynamic, Some(DynamicMesh::Particles(_))))
        .unwrap();
    let dynamic = mesh.dynamic.as_ref().unwrap();
    let mut vertices = mesh.vertices.clone();
    let mut state = ShipState::default();
    assert!(!dynamic.visible(&state, 2.0));
    state.fire_started = Some(0.0);
    dynamic
        .deform(
            &mut vertices,
            2.0,
            SailWind::default(),
            Vec3::new(10.0, 10.0, 10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(
        vertices
            .iter()
            .any(|v| v.color[3] > 0.0 && v.color[3] < 1.0)
    );
    for quad in vertices.chunks_exact(4) {
        let n = Vec3::from_array(quad[0].normal);
        let edge = Vec3::from_array(quad[1].position) - Vec3::from_array(quad[0].position);
        assert!(n.dot(edge).abs() < 1e-4);
    }
    assert!(dynamic.visible(&state, 2.0));
    state.fire_started = None;
    assert!(!dynamic.visible(&state, 2.0));
}

#[test]
fn native_emissions_select_one_locator_and_fade_after_stop_without_restarting() {
    let asset = ModelAsset::ship_demo().unwrap();
    let first = asset.ship.fires[0].clone();
    let mut second = first.clone();
    second.position.x += 100.0;
    let patch = ParticlePatch {
        points: std::sync::Arc::new(vec![first, second]),
        smoke: true,
        per_point: 20,
        lifetime: 7.0,
        style: None,
    };
    let dynamic = DynamicMesh::Particles(patch);
    let mut vertices = vec![ModelVertex::zeroed(); dynamic.vertex_count()];
    let mut state = ShipState {
        fire_started: Some(0.0),
        native_fire: Some(vec![FireEmission {
            instance: 0,
            point: 1,
            smoke: true,
            started: 0.0,
            stopped: Some(2.0),
            profile: None,
        }]),
        ..Default::default()
    };
    vertices.resize(
        dynamic.frame_vertex_count(&state, 2.5).unwrap(),
        bytemuck::Zeroable::zeroed(),
    );
    dynamic
        .deform(
            &mut vertices,
            2.5,
            SailWind::default(),
            Vec3::new(10.0, 10.0, 10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(vertices.iter().any(|v| v.color[3] > 0.0));
    assert!(
        vertices
            .iter()
            .filter(|v| v.color[3] > 0.0)
            .all(|v| v.position[0] > 50.0)
    );
    vertices.resize(
        dynamic.frame_vertex_count(&state, 9.5).unwrap(),
        bytemuck::Zeroable::zeroed(),
    );
    dynamic
        .deform(
            &mut vertices,
            9.5,
            SailWind::default(),
            Vec3::new(10.0, 10.0, 10.0),
            &asset.ship,
            &state,
        )
        .unwrap();
    assert!(vertices.iter().all(|v| v.color[3] == 0.0));
    assert!(!dynamic.visible(&state, 9.5));
    state.native_fire = Some(vec![]);
    assert!(!dynamic.visible(&state, 2.5));
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn native_fire_emission_renders_and_cleanup_restores_the_unlit_frame() {
    let mut asset = ModelAsset::ship_demo().unwrap();
    asset
        .meshes
        .retain(|m| matches!(m.dynamic, Some(DynamicMesh::Particles(_))));
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    camera.target = asset.ship.fires[0].position;
    camera.distance = 10.0;
    camera.yaw = 0.4;
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
                SailWind {
                    speed: 0.0,
                    direction: 0.0,
                },
            )
        })
        .unwrap()
    };
    let unlit = capture(ShipState::default());
    let active = capture(ShipState {
        native_fire: Some(vec![
            FireEmission {
                instance: 0,
                point: 0,
                smoke: true,
                started: 0.0,
                stopped: None,
                profile: None,
            },
            FireEmission {
                instance: 0,
                point: 0,
                smoke: false,
                started: 0.0,
                stopped: None,
                profile: None,
            },
        ]),
        ..Default::default()
    });
    assert_ne!(unlit.rgba, active.rgba);
    let clear = capture(ShipState {
        native_fire: Some(vec![]),
        fire_started: Some(0.0),
        ..Default::default()
    });
    assert_eq!(unlit.rgba, clear.rgba);
}

fn mask_red(frame: &crate::Frame) -> Vec<u8> {
    frame
        .rgba
        .chunks_exact(4)
        .map(|p| {
            if p[0] > 100 && p[1] < 20 && p[2] < 20 {
                p[0]
            } else {
                0
            }
        })
        .collect()
}
#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn crew_reuses_gpu_buffers_moves_and_can_be_hidden_and_restored_on_pause() {
    let mut asset = ModelAsset::ship_demo().unwrap();
    asset
        .meshes
        .retain(|m| matches!(m.dynamic, Some(DynamicMesh::Crew(_))));
    asset.textures[1].mips[0].rgba = vec![255, 0, 0, 255];
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    camera.target = Vec3::new(0.0, 2.0, 0.0);
    camera.distance = 14.0;
    camera.yaw = 0.4;
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let mut capture = |time, visible| {
        scene
            .set_ship_state(ShipState {
                crew_visible: visible,
                ..Default::default()
            })
            .unwrap();
        crate::render_offscreen_with(&gpu, 321, 181, time, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                time,
                &camera,
                SailWind {
                    speed: 0.0,
                    direction: 0.0,
                },
            )
        })
        .unwrap()
    };
    let first = capture(0.0, true);
    assert!(mask_red(&first).into_iter().filter(|&r| r > 0).count() > 30);
    assert_ne!(mask_red(&first), mask_red(&capture(0.4, true)));
    assert!(mask_red(&capture(0.0, false)).into_iter().all(|r| r == 0));
    assert_eq!(first.rgba, capture(0.0, true).rgba);
}
#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn mast_hull_and_alpha_particles_change_one_persistent_scene_and_repair_exactly() {
    let asset = ModelAsset::ship_demo().unwrap();
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    camera.yaw = 0.4;
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let mut capture = |state| {
        scene.set_ship_state(state).unwrap();
        crate::render_offscreen_with(&gpu, 321, 181, 0.0, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                0.0,
                &camera,
                SailWind {
                    speed: 0.0,
                    direction: 0.0,
                },
            )
        })
        .unwrap()
    };
    let first = capture(ShipState::default());
    let mut state = ShipState::default();
    state.broken_masts.insert(2, -2.0);
    assert_ne!(first.rgba, capture(state).rgba);
    let mut state = ShipState::default();
    state.broken_hulls.insert("shatter1".into(), -1.0);
    assert_ne!(first.rgba, capture(state).rgba);
    let fire = capture(ShipState {
        fire_started: Some(-2.0),
        ..Default::default()
    });
    assert_ne!(first.rgba, fire.rgba);
    assert!(fire.rgba.chunks_exact(4).all(|p| p[3] == 255));
    assert_eq!(first.rgba, capture(ShipState::default()).rgba);
}
