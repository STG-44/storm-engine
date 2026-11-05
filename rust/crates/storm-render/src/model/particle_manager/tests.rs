use super::*;
fn library() -> Arc<ParticleLibrary> {
    let bytes =
        include_bytes!("../../../../storm-formats/reference/particle_project/billboard.xps");
    Arc::new(
        ParticleLibrary::from_bytes([
            ("first".into(), bytes.to_vec()),
            ("second".into(), bytes.to_vec()),
        ])
        .unwrap(),
    )
}
#[test]
fn project_systems_prepare_on_demand_and_release_without_changing_other_owners() {
    let library = library();
    assert_eq!(library.prepared_count().unwrap(), 0);
    let mut m = ParticleManager::new(library.clone());
    assert_eq!(m.create(1, "missing", 0, false).unwrap(), None);
    assert_eq!(m.create(1, "FIRST.XPS", 0, false).unwrap(), Some(1));
    assert_eq!(m.create(2, "first", 0, true).unwrap(), Some(2));
    assert_eq!(m.create(1, "second", 1, true).unwrap(), Some(3));
    assert_eq!(library.prepared_count().unwrap(), 2);
    m.pause(1);
    m.advance(1.0).unwrap();
    m.pause(1);
    assert_eq!(m.emissions(1)[0].stopped, Some(0.0));
    m.release_owner(1);
    assert_eq!(m.ids(), [2]);
    assert_eq!(m.emissions(2)[0].stopped, None);
    m.advance(2.0).unwrap();
    assert!(m.validate(2));
    assert!(!m.validate(1));
    assert!(!m.release(1));
    assert_eq!(m.create(2, "second", 0, false).unwrap(), Some(4));
    let mut asset = crate::model::ModelAsset::ship_demo().unwrap();
    asset.ship.particle_library = Some(library.clone());
    let mut state = asset.ship_state.clone();
    state.native_fire = Some(m.emissions(2));
    state.validate(&asset.ship).unwrap();
    state.native_fire.as_mut().unwrap()[0].profile = Some("FIRST.XPS".into());
    assert!(
        state.validate(&asset.ship).is_err(),
        "raw aliases must not silently miss canonical render keys"
    );
    m.clear();
    assert!(m.is_empty());
    assert_eq!(m.create(2, "first", 0, false).unwrap(), Some(5));
    assert!(m.advance(f32::NAN).is_err());
    assert!(m.advance(1.0).is_err());
}
#[test]
fn release_uses_native_swap_remove_order_and_library_reuse_keeps_replay_private() {
    let library = library();
    let mut a = ParticleManager::new(library.clone());
    let mut b = ParticleManager::new(library);
    for _ in 0..4 {
        a.create(1, "first", 0, false).unwrap();
    }
    assert!(a.release(2));
    assert_eq!(a.ids(), [1, 4, 3]);
    b.create(2, "first", 0, false).unwrap();
    b.pause(1);
    a.advance(1.0).unwrap();
    b.advance(1.0).unwrap();
    assert!(a.emissions(1).iter().all(|e| e.stopped.is_none()));
    assert_eq!(b.emissions(2)[0].stopped, Some(0.0));
}
#[test]
fn unsupported_data_does_not_prepare_a_partial_profile_or_consume_an_identity() {
    let bytes = include_bytes!("../../../../storm-formats/reference/particles/synthetic.xps");
    let library =
        Arc::new(ParticleLibrary::from_bytes([("mixed".into(), bytes.to_vec())]).unwrap());
    let mut m = ParticleManager::new(library.clone());
    assert!(m.create(1, "mixed", 0, false).is_err());
    assert!(m.is_empty());
    assert_eq!(library.prepared_count().unwrap(), 0);
    assert!(m.create(0, "mixed", 0, false).is_err());
    assert!(ParticleLibrary::from_bytes([("../bad".into(), bytes.to_vec())]).is_err());
}

#[test]
fn registry_order_validity_and_clear_match_complete_native_manager_snapshots() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../reference/particle_manager/cases.json"
    ))
    .unwrap();
    let mut manager = ParticleManager::new(library());
    let mut created = 0u64;
    for (action, expected) in fixture["actions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["snapshots"].as_array().unwrap())
    {
        let mut released = Vec::new();
        match action[0].as_str().unwrap() {
            "create" => {
                created += 1;
                assert_eq!(manager.create(1, "first", 0, false).unwrap(), Some(created));
            }
            "release" => {
                let id = action[1].as_u64().unwrap();
                if manager.release(id) {
                    released.push(id);
                }
            }
            "clear" => {
                released = manager.ids();
                manager.clear();
            }
            _ => unreachable!(),
        }
        let actual = serde_json::json!({"ids":manager.ids(),"valid":(1..=created).map(|id|manager.validate(id)).collect::<Vec<_>>(),"released":released});
        assert_eq!(&actual, expected);
    }
    for (input, expected) in fixture["alive_inputs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["alive"].as_array().unwrap())
    {
        let emitters = input
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e[0].as_bool().unwrap(), e[1].as_u64().unwrap() as u32));
        assert_eq!(
            storm_sim::particle_emitter::System::is_alive(emitters),
            expected.as_bool().unwrap()
        );
    }
}
#[test]
fn paused_looping_emitters_remain_valid_and_completed_one_shots_release() {
    use storm_formats::particles::Value;
    let mut library = library();
    for (key, profile) in &mut Arc::get_mut(&mut library).unwrap().profiles {
        for emitter in &mut profile.emitters {
            for field in &mut emitter.fields.0 {
                if field.name.matches("Looping") {
                    field.value = Value::Bool(key == "first.xps");
                }
                if field.name.matches("Life time") {
                    field.value = Value::Float(0.25);
                }
            }
            for particle in &mut emitter.particles {
                for field in &mut particle.fields.0 {
                    if field.name.matches("Life time")
                        && let Value::Graph(g) = &mut field.value
                    {
                        for point in g.min.iter_mut().chain(&mut g.max) {
                            point.value = 0.1;
                        }
                    }
                }
            }
        }
    }
    let mut m = ParticleManager::new(library);
    m.create(1, "first", 0, false).unwrap();
    m.create(2, "second", 0, false).unwrap();
    m.pause(1);
    m.advance(1.0).unwrap();
    assert!(m.validate(1));
    assert!(!m.validate(2));
    assert_eq!(m.create(2, "second", 0, false).unwrap(), Some(3));
    m.release_owner(1);
    assert!(!m.validate(1));
}
#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn lazy_profiles_draw_on_both_ships_and_release_keeps_the_survivor_pixels() {
    use crate::model::{DynamicMesh, ModelAsset, ModelPose, ModelScene, OrbitCamera, SailWind};
    use crate::{Gpu, create_instance, render_fleet_offscreen};
    let library = library();
    let mut manager = ParticleManager::new(library.clone());
    let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut scenes = Vec::new();
    let mut states = Vec::new();
    let mut camera = None;
    for index in 0..2 {
        let mut asset = ModelAsset::ship_demo().unwrap();
        asset.ship.particle_texture = asset.meshes.iter().find_map(|m| {
            matches!(m.dynamic, Some(DynamicMesh::Particles(_))).then_some(m.texture)
        });
        asset
            .meshes
            .retain(|m| !matches!(m.dynamic, Some(DynamicMesh::Particles(_))));
        asset.ship.particle_library = Some(library.clone());
        asset.ship_state.native_fire = Some(Vec::new());
        let mut c = OrbitCamera::new(&asset);
        c.yaw = 0.0;
        c.pitch = 0.3;
        c.distance = 55.0;
        c.radius = 35.0;
        c.target.x = 0.0;
        camera = Some(c);
        let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
        scene
            .set_pose(ModelPose {
                translation: [if index == 0 { -12.0 } else { 12.0 }, 0.0, 0.0],
                ..Default::default()
            })
            .unwrap();
        // Seed birth history before the first draw (all subsequent draws use t=1).
        scene
            .particle_history
            .record(scene.pose.sample(0.0))
            .unwrap();
        scenes.push(scene);
        states.push(asset.ship_state);
    }
    let camera = camera.unwrap();
    let draw = |scenes: &mut [ModelScene]| {
        render_fleet_offscreen(&gpu, scenes, &camera, SailWind::default(), 641, 361, 1.0)
            .unwrap()
            .rgba
    };
    let baseline = draw(&mut scenes);
    manager.create(1, "first", 0, false).unwrap();
    states[0].native_fire = Some(manager.emissions(1));
    scenes[0].set_ship_state(states[0].clone()).unwrap();
    let first = draw(&mut scenes);
    manager.create(2, "second", 0, false).unwrap();
    states[1].native_fire = Some(manager.emissions(2));
    scenes[1].set_ship_state(states[1].clone()).unwrap();
    let both = draw(&mut scenes);
    assert_eq!(draw(&mut scenes), both);
    manager.release_owner(1);
    states[0].native_fire = Some(manager.emissions(1));
    scenes[0].set_ship_state(states[0].clone()).unwrap();
    let survivor = draw(&mut scenes);
    let differences = |a: &[u8], b: &[u8], left: bool| {
        a.chunks_exact(4)
            .zip(b.chunks_exact(4))
            .enumerate()
            .filter(|(i, (a, b))| (*i % 641 < 320) == left && a != b)
            .count()
    };
    assert!(differences(&first, &baseline, true) > 2);
    assert_eq!(differences(&first, &baseline, false), 0);
    assert!(differences(&both, &first, false) > 2);
    assert_eq!(differences(&both, &first, true), 0);
    assert_eq!(differences(&survivor, &baseline, true), 0);
    assert_eq!(differences(&survivor, &both, false), 0);
}

#[test]
fn capacity_rejection_consumes_no_identity_and_explicit_release_restores_capacity() {
    let mut manager = ParticleManager::new(library());
    for id in 1..=MAX_PARTICLE_INSTANCES as u64 {
        assert_eq!(manager.create(1, "first", 0, false).unwrap(), Some(id));
    }
    assert_eq!(manager.create(2, "first", 0, false).unwrap(), None);
    assert!(manager.release(1));
    assert_eq!(
        manager.create(2, "second", 0, false).unwrap(),
        Some(MAX_PARTICLE_INSTANCES as u64 + 1)
    );
    assert!(!manager.validate(1));
    manager.release_owner(1);
    assert_eq!(manager.owners(), BTreeSet::from([2]));
}
