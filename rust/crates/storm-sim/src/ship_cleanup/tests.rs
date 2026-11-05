use super::*;
use serde_json::{Value as Json, json};
use storm_core::{EntityDestroy, EntityManager, LayerType, Value};
#[derive(Default)]
struct State {
    events: Vec<Json>,
    active: [EntityId; 2],
    tracks: Vec<EntityId>,
}
struct Port<'a> {
    manager: &'a mut EntityManager,
    state: Rc<RefCell<State>>,
}
impl CleanupPort for Port<'_> {
    fn erase(&mut self, id: EntityId) -> Result<()> {
        self.manager.erase(id);
        let [ship, model] = self.state.borrow().active;
        let row = json!([
            "erase",
            id.raw(),
            ship.raw(),
            self.manager.get(ship).is_some(),
            self.manager.is_valid(ship),
            self.manager.get(model).is_some(),
            self.manager.is_valid(model)
        ]);
        self.state.borrow_mut().events.push(row);
        Ok(())
    }
    fn send(&mut self, service: EntityId, code: i32, owner: EntityId) -> Result<()> {
        let model = self.state.borrow().active[1];
        self.state.borrow_mut().events.push(json!([
            "send",
            service.raw(),
            code,
            owner.raw(),
            self.manager.is_valid(service),
            self.manager.get(model).is_some(),
            self.manager.is_valid(model)
        ]));
        self.manager.send_message(
            service,
            &mut Message::new("li", vec![Value::Long(code), Value::Entity(owner)])?,
        )?;
        Ok(())
    }
    fn tracks(&mut self) -> Option<EntityId> {
        let id = self.manager.ids("ShipTracks").first().copied()?;
        self.state
            .borrow_mut()
            .events
            .push(json!(["track_lookup", id.raw()]));
        self.manager.get(id).map(|_| id)
    }
    fn del_ship(&mut self, _: EntityId, ship: EntityId) -> Result<()> {
        let mut s = self.state.borrow_mut();
        if let Some(i) = s.tracks.iter().position(|v| *v == ship) {
            s.events.push(json!(["track_drop", ship.raw()]));
            s.tracks.remove(i);
        }
        Ok(())
    }
}
struct Probe {
    state: Rc<RefCell<State>>,
    id: Rc<std::cell::Cell<EntityId>>,
    kind: u32,
    child: EntityId,
    links: Option<Links>,
}
impl Entity for Probe {
    fn destroy(&mut self, c: &mut EntityDestroy<'_>) -> Result<()> {
        if let Some(links) = self.links {
            self.state.borrow_mut().active = [c.id, links.model];
            release(
                c.id,
                links,
                &mut Port {
                    manager: c.manager,
                    state: self.state.clone(),
                },
            )?;
        } else if self.child != EntityId::INVALID {
            Port {
                manager: c.manager,
                state: self.state.clone(),
            }
            .erase(self.child)?;
        }
        Ok(())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.state
            .borrow_mut()
            .events
            .push(json!(["drop", self.id.get().raw(), self.kind]));
    }
}
fn create(
    manager: &mut EntityManager,
    state: &Rc<RefCell<State>>,
    history: &mut Vec<EntityId>,
    name: &str,
    kind: u32,
    child: EntityId,
    links: Option<Links>,
) -> EntityId {
    let cell = Rc::new(std::cell::Cell::new(EntityId::INVALID));
    let id = manager
        .insert(
            name,
            Probe {
                state: state.clone(),
                id: cell.clone(),
                kind,
                child,
                links,
            },
        )
        .unwrap();
    cell.set(id);
    history.push(id);
    id
}
#[test]
fn full_native_ship_destructor_group_marks_and_nested_registry_cleanup_match_cpp() {
    struct Empty;
    impl Entity for Empty {}
    let fixture: Json =
        serde_json::from_str(include_str!("../../reference/ship_cleanup/cases.json")).unwrap();
    let names = ["sail", "rope", "flag", "vant", "vantl", "vantz"];
    let codes = [
        MSG_SAIL_DEL_GROUP,
        MSG_ROPE_DEL_GROUP,
        MSG_FLAG_DEL_GROUP,
        MSG_VANT_DEL_GROUP,
        MSG_VANT_DEL_GROUP,
        MSG_VANT_DEL_GROUP,
    ];
    for case in fixture["cases"].as_array().unwrap() {
        let profile = case["profile"].as_u64().unwrap() as usize;
        let mut m = EntityManager::default();
        m.set_layer_type(1, LayerType::Execute).unwrap();
        m.set_layer_type(0, LayerType::Realize).unwrap();
        let state = Rc::new(RefCell::new(State::default()));
        let groups: Vec<_> = (0..6)
            .map(|_| Rc::new(RefCell::new(Groups::default())))
            .collect();
        let mut history = Vec::new();
        let mut services = [EntityId::INVALID; 6];
        for i in 0..6 {
            if profile & (1 << i) != 0 {
                services[i] = m
                    .insert(
                        names[i],
                        GroupService {
                            code: codes[i],
                            groups: groups[i].clone(),
                        },
                    )
                    .unwrap();
                history.push(services[i]);
            }
        }
        if !profile.is_multiple_of(3) {
            history.push(m.insert("ShipTracks", Empty).unwrap());
        }
        let bonus = create(
            &mut m,
            &state,
            &mut history,
            "probe",
            3,
            EntityId::INVALID,
            None,
        );
        let mut ships = Vec::new();
        let mut models = Vec::new();
        for i in 0..2 {
            let child = if i == 0 && profile.is_multiple_of(2) {
                bonus
            } else {
                EntityId::INVALID
            };
            let model = create(&mut m, &state, &mut history, "model", 1, child, None);
            let blots = create(
                &mut m,
                &state,
                &mut history,
                "blots",
                2,
                EntityId::INVALID,
                None,
            );
            let links = Links {
                model,
                blots,
                sail: services[0],
                rope: services[1],
                flag: services[2],
                vants: [services[3], services[4], services[5]],
            };
            let ship = create(
                &mut m,
                &state,
                &mut history,
                "ship",
                0,
                EntityId::INVALID,
                Some(links),
            );
            m.add_to_layer(1, ship, 2).unwrap();
            m.add_to_layer(0, ship, 31).unwrap();
            for (k, g) in groups.iter().enumerate() {
                g.borrow_mut()
                    .owners
                    .push((if k == 0 { ship } else { model }, false));
            }
            if !profile.is_multiple_of(3) {
                state.borrow_mut().tracks.push(ship);
            }
            ships.push(ship);
            models.push(model);
        }
        for expected in case["snapshots"].as_array().unwrap() {
            let step = expected["step"].as_u64().unwrap();
            match step {
                1 => {
                    let mut p = Port {
                        manager: &mut m,
                        state: state.clone(),
                    };
                    p.send(services[0], MSG_SAIL_DEL_GROUP, models[0]).unwrap();
                    p.send(services[1], MSG_ROPE_DEL_GROUP, ships[0]).unwrap();
                }
                2 => m.erase(ships[0]),
                3 | 7 => m.new_lifecycle().unwrap(),
                4 => {
                    m.new_lifecycle().unwrap();
                    m.erase(ships[0]);
                }
                5 => {
                    create(
                        &mut m,
                        &state,
                        &mut history,
                        "probe",
                        3,
                        EntityId::INVALID,
                        None,
                    );
                }
                6 => {
                    if profile & 32 != 0 {
                        m.erase(services[2]);
                    }
                    m.erase(ships[1]);
                }
                _ => {}
            }
            let actual = json!({"step":step,"entities":history.iter().map(|&id|json!([id.raw(),m.is_valid(id),m.get(id).is_some()])).collect::<Vec<_>>(),
                "groups":groups.iter().map(|g|g.borrow().owners.iter().map(|(id,deleted)|json!([id.raw(),deleted])).collect::<Vec<_>>()).collect::<Vec<_>>(),"pending":groups.iter().map(|g|g.borrow().pending).collect::<Vec<_>>(),"tracks":state.borrow().tracks.iter().map(|id|id.raw()).collect::<Vec<_>>(),
                "execute":m.layer_ids(LayerType::Execute).iter().map(|id|id.raw()).collect::<Vec<_>>(),"realize":m.layer_ids(LayerType::Realize).iter().map(|id|id.raw()).collect::<Vec<_>>(),"events":state.borrow().events});
            assert_eq!(&actual, expected, "profile {profile} step {step}");
        }
    }
}
#[test]
fn group_deletion_marks_only_first_matching_owner_and_retains_storage_until_service_update() {
    let owner = EntityId::from_raw(7);
    let mut groups = Groups {
        owners: vec![(owner, false), (owner, false)],
        pending: false,
    };
    groups.mark_deleted(EntityId::from_raw(8));
    assert!(!groups.pending);
    groups.mark_deleted(owner);
    groups.mark_deleted(owner);
    assert_eq!(groups.owners, vec![(owner, true), (owner, false)]);
    assert!(groups.pending);
}
