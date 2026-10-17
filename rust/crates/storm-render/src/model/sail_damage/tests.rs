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
fn traces_use_deformed_world_space_cloth_pass_through_holes_and_reject_invalid_state() {
    let mut asset = ModelAsset::sail_demo().unwrap();
    asset.meshes.truncate(1);
    let patch = asset.meshes[0].sail.as_ref().unwrap();
    let wind = SailWind {
        speed: 16.0,
        direction: 0.0,
    };
    let time = 2.0;
    let vertices = patch.vertices(wind, time).unwrap();
    let n = 7 * 13 + 2; // first column, third vertical sector center
    let p = Vec3::from_array(vertices[n].position);
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
    assert!(patch.cutout(hits[0].uv, state.sail_holes[&0]));
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
            .unwrap()
            .is_empty()
    );
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
