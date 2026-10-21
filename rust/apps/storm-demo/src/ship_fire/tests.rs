use super::*;
use storm_sim::fire::FirePlace;
#[test]
fn retained_native_factory_and_pause_snapshots_keep_same_profile_instances_independent() {
    let fixture: Json = serde_json::from_str(include_str!(
        "../../reference/particle_instances/cases.json"
    ))
    .unwrap();
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    let mut effects = PreviewEffects {
        world: world.clone(),
        entity: EntityId::from_raw(1),
    };
    for (action, expected) in fixture["actions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["snapshots"].as_array().unwrap())
    {
        if action[0] == "create" {
            effects
                .create_particle(ParticleKind::Smoke, "same", 0, [0.0; 3])
                .unwrap();
        } else {
            effects.pause_particle(ParticleId(action[1].as_u64().unwrap()));
        }
        let actual: Vec<_> = world
            .borrow()
            .emissions()
            .iter()
            .map(|e| json!({"instance":e.instance,"paused":e.stopped.is_some()}))
            .collect();
        assert_eq!(json!(actual), *expected);
    }
}
#[test]
fn restart_preserves_same_time_instances_in_both_channels_until_their_tails_expire() {
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    let mut effects = PreviewEffects {
        world: world.clone(),
        entity: EntityId::from_raw(1),
    };
    let mut fire = FirePlace::new();
    fire.run("same", "same", 7, "sound", 1.0, 0, [0.0; 3], &mut effects);
    fire.stop(&mut effects);
    fire.run("same", "same", 9, "sound", 1.0, 0, [0.0; 3], &mut effects);
    let emissions = world.borrow().emissions();
    assert_eq!(
        emissions.iter().map(|e| e.instance).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert!(emissions[..2].iter().all(|e| e.stopped == Some(0.0)));
    assert!(emissions[2..].iter().all(|e| e.stopped.is_none()));
    world.borrow_mut().set_clock(7.0).unwrap();
    assert_eq!(
        world
            .borrow()
            .emissions()
            .iter()
            .map(|e| e.instance)
            .collect::<Vec<_>>(),
        [3, 4]
    );
    fire.stop(&mut effects);
    world.borrow_mut().set_clock(14.0).unwrap();
    assert!(world.borrow().emissions().is_empty());
}
#[test]
fn instance_budget_returns_null_without_consuming_an_id_and_recovers_after_cleanup() {
    let mut service = EffectsWorld::default();
    let limit = storm_render::model::MAX_PARTICLE_INSTANCES as u64;
    for id in 1..=limit {
        service.particles.insert(
            ParticleId(id),
            FireEmission {
                instance: id,
                point: 0,
                smoke: true,
                started: 0.0,
                stopped: Some(0.0),
                profile: None,
            },
        );
    }
    service.next_id = limit;
    let world = Rc::new(RefCell::new(service));
    let mut effects = PreviewEffects {
        world: world.clone(),
        entity: EntityId::from_raw(1),
    };
    assert!(
        effects
            .create_particle(ParticleKind::Smoke, "s", 0, [0.0; 3])
            .is_none()
    );
    assert_eq!(world.borrow().next_id, limit);
    assert_eq!(
        world.borrow().report()["recent"][0]["reason"],
        "instance_budget"
    );
    world.borrow_mut().set_clock(7.0).unwrap();
    assert_eq!(
        effects.create_particle(ParticleKind::Smoke, "s", 0, [0.0; 3]),
        Some(ParticleId(limit + 1))
    );
}
#[test]
fn cached_profiles_preserve_partial_creation_failure_and_native_active_run_rules() {
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    world.borrow_mut().set_profiles(Some(BTreeSet::from([
        "ship_fire.xps".into(),
        "ship_smoke.xps".into(),
    ])));
    let mut effects = PreviewEffects {
        world: world.clone(),
        entity: EntityId::from_raw(1),
    };
    let mut fire = FirePlace::new();
    fire.run(
        "SHIP_FIRE.XPS",
        "missing",
        7,
        "sound",
        1.0,
        0,
        [0.0; 3],
        &mut effects,
    );
    assert!(fire.is_active());
    let emissions = world.borrow().emissions();
    assert_eq!(emissions.len(), 1);
    assert_eq!(emissions[0].profile.as_deref(), Some("ship_fire.xps"));
    assert!(
        emissions[0].smoke,
        "profile name, rather than channel, selects the visual"
    );
    assert_eq!(
        world.borrow().next_id,
        1,
        "a missing profile must not allocate an ID"
    );
    let total = world.borrow().total;
    fire.run(
        "Ship_Smoke",
        "Ship_Fire",
        9,
        "other",
        99.0,
        0,
        [1.0; 3],
        &mut effects,
    );
    assert_eq!(
        world.borrow().total,
        total,
        "active Run ignores all replacement names"
    );
    world.borrow_mut().set_clock(1.0).unwrap();
    fire.stop(&mut effects);
    assert_eq!(world.borrow().emissions()[0].stopped, Some(1.0));
    let report = world.borrow().report();
    let events = report["recent"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["op"] == "pause_particle")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["reason"] == "profile_not_cached")
            .count(),
        1
    );
    fire.run(
        "../ship_fire",
        "Ship_Smoke",
        11,
        "sound",
        1.0,
        0,
        [0.0; 3],
        &mut effects,
    );
    assert_eq!(world.borrow().next_id, 2);
    assert!(
        world
            .borrow()
            .emissions()
            .iter()
            .any(|e| e.profile.as_deref() == Some("ship_smoke.xps"))
    );
}

#[test]
fn configured_manual_toggle_uses_original_cached_names() {
    let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    world.borrow_mut().set_profiles(Some(BTreeSet::from([
        "ship_fire.xps".into(),
        "ship_smoke.xps".into(),
    ])));
    let controller = Rc::new(RefCell::new(Controller::new(
        asset.ship.fires.clone(),
        PreviewEffects {
            world: world.clone(),
            entity: EntityId::from_raw(1),
        },
    )));
    let mut session = Session {
        controller: Some(controller),
        world,
    };
    assert!(session.toggle().unwrap());
    assert_eq!(session.world.borrow().emissions().len(), 2);
    assert!(
        session.report()["emissions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["profile"].as_str().unwrap().starts_with("ship_"))
    );
    assert!(session.toggle().unwrap());
    assert!(
        session
            .world
            .borrow()
            .emissions()
            .iter()
            .all(|e| e.stopped == Some(0.0))
    );
}
#[test]
fn particle_service_pauses_emission_without_deleting_living_particles_and_records_silent_sound_requests()
 {
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    let mut effects = PreviewEffects {
        world: world.clone(),
        entity: EntityId::from_raw(1),
    };
    let mut fire = FirePlace::new();
    fire.run("smoke", "fire", 7, "sound", 1.0, 0, [1.0; 3], &mut effects);
    assert_eq!(world.borrow().emissions().len(), 2);
    assert_eq!(fire.sound(), 0);
    world.borrow_mut().set_clock(1.0).unwrap();
    fire.stop(&mut effects);
    assert!(
        world
            .borrow()
            .emissions()
            .iter()
            .all(|e| e.stopped == Some(1.0))
    );
    assert_eq!(world.borrow().report()["sound_playback"], false);
    world.borrow_mut().set_clock(8.0).unwrap();
    assert!(world.borrow().emissions().is_empty());
}
#[test]
fn controller_drop_stops_active_emitters_but_the_shared_particle_service_survives() {
    let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    let mut controller = Controller::new(
        asset.ship.fires.clone(),
        PreviewEffects {
            world: world.clone(),
            entity: EntityId::from_raw(1),
        },
    );
    let slot = &mut controller.queries.fire_places_mut()[0];
    slot.state
        .run("s", "f", 7, "a", 1.0, 0, [0.0; 3], &mut controller.effects);
    drop(controller);
    assert_eq!(world.borrow().emissions().len(), 2);
    assert!(
        world
            .borrow()
            .emissions()
            .iter()
            .all(|e| e.stopped == Some(0.0))
    );
}
#[test]
fn native_preview_controls_stop_restart_and_reset_without_repopulating_safe_deleted_records() {
    let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
    let world = Rc::new(RefCell::new(EffectsWorld::default()));
    let controller = Rc::new(RefCell::new(Controller::new(
        asset.ship.fires.clone(),
        PreviewEffects {
            world: world.clone(),
            entity: EntityId::from_raw(1),
        },
    )));
    let mut session = Session {
        controller: Some(controller.clone()),
        world,
    };
    assert!(session.toggle().unwrap());
    assert!(
        controller.borrow().queries.fire_places()[0]
            .state
            .is_active()
    );
    assert!(session.toggle().unwrap());
    assert!(
        !controller.borrow().queries.fire_places()[0]
            .state
            .is_active()
    );
    assert!(session.toggle().unwrap());
    session.reset();
    assert!(
        !controller.borrow().queries.fire_places()[0]
            .state
            .is_active()
    );
    assert!(session.world.borrow().emissions().is_empty());
    controller.borrow_mut().clear();
    assert!(!session.toggle().unwrap());
}
#[test]
fn loaded_profile_lifetimes_keep_native_tails_beyond_the_legacy_seven_seconds() {
    let mut world = EffectsWorld::default();
    world.set_retention(15.0).unwrap();
    world.particles.insert(
        ParticleId(1),
        FireEmission {
            instance: 0,
            point: 0,
            smoke: true,
            started: 0.0,
            stopped: Some(1.0),
            profile: None,
        },
    );
    world.set_clock(8.0).unwrap();
    assert_eq!(world.emissions().len(), 1);
    world.set_clock(16.0).unwrap();
    assert!(world.emissions().is_empty());
    assert!(world.set_retention(f32::INFINITY).is_err());
    assert!(world.set_retention(61.0).is_err());
}
