use super::*;
use storm_sim::fire::FirePlace;
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
