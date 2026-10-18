//! Native observer for original cannon/sail event payloads. No game scripts run here.
use anyhow::{Result, bail};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::VecDeque, path::Path, rc::Rc};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityId, EntityManager, EventDispatcher, Message, Value,
};
use storm_render::model::{ModelAsset, SailImpact, ShipState};

#[derive(Default)]
struct Records {
    total: u64,
    recent: VecDeque<Json>,
    attribute_notifications: u64,
}
struct Observer {
    records: Rc<RefCell<Records>>,
    attributes: Rc<RefCell<AttributeStore>>,
    root: AttributeId,
}
impl Entity for Observer {
    fn process_message(&mut self, m: &mut Message) -> Result<u64> {
        let mut changed = None;
        let record = match m.format() {
            "llssllllf" => {
                let owner = m.long()?;
                let character = m.long()?;
                let model = m.string()?.to_owned();
                let node = m.string()?.to_owned();
                let group = m.long()?;
                let count = m.long()?;
                let mask = m.long()?;
                let maximum = m.long()?;
                let fraction = m.float()?;
                let mut attributes = self.attributes.borrow_mut();
                let hd = attributes.create_path(
                    self.root,
                    &format!("characters.{character}.ship.sails.{node}.{group}.hd"),
                )?;
                attributes.set_value(hd, Some(&mask.to_string()))?;
                changed = Some(hd);
                json!({"name":"DoSailHole", "format":"llssllllf", "owner":owner,
                    "character":character,"model":model,"node":node,"group":group,
                    "hole_count":count,"hole_mask":mask,"max_holes":maximum,"sail_weight_fraction":fraction})
            }
            "lfffl" => json!({"name":"ShpSailDamage", "format":"lfffl", "character":m.long()?,
                "position":[m.float()?,m.float()?,m.float()?],"owner":m.long()?}),
            _ => bail!("unsupported native damage observer message"),
        };
        let mut records = self.records.borrow_mut();
        records.total += 1;
        if records.recent.len() == 256 {
            records.recent.pop_front();
        }
        records.recent.push_back(record);
        Ok(changed.map_or(0, AttributeId::raw))
    }
    fn attribute_changed(
        &mut self,
        attributes: &AttributeStore,
        changed: AttributeId,
    ) -> Result<u32> {
        anyhow::ensure!(
            attributes.name(changed) == Some("hd"),
            "unexpected damage attribute notification"
        );
        self.records.borrow_mut().attribute_notifications += 1;
        Ok(0)
    }
}

pub struct DamageEvents {
    dispatcher: EventDispatcher,
    records: Rc<RefCell<Records>>,
    observer: EntityId,
    attributes: Rc<RefCell<AttributeStore>>,
    root: AttributeId,
}
impl DamageEvents {
    pub fn new() -> Result<Self> {
        let records = Rc::new(RefCell::new(Records::default()));
        let mut attributes = AttributeStore::default();
        let root = attributes.create_root()?;
        let attributes = Rc::new(RefCell::new(attributes));
        let mut entities = EntityManager::default();
        let observer = entities.insert(
            "preview_sail_damage_observer",
            Observer {
                records: records.clone(),
                attributes: attributes.clone(),
                root,
            },
        )?;
        entities.bind_attributes(observer, &attributes.borrow(), Some(root))?;
        let entities = Rc::new(RefCell::new(entities));
        let mut dispatcher = EventDispatcher::default();
        for name in ["DoSailHole", "ShpSailDamage"] {
            let entities = entities.clone();
            let attributes = attributes.clone();
            dispatcher.register(name, move |message| {
                let changed = entities.borrow_mut().send_message(observer, message)?;
                if changed != 0 {
                    entities.borrow_mut().notify_attribute_changed(
                        observer,
                        &attributes.borrow(),
                        AttributeId::from_raw(changed),
                    )?;
                }
                Ok(None)
            })?;
        }
        Ok(Self {
            dispatcher,
            records,
            observer,
            attributes,
            root,
        })
    }
    /// One swept segment: per-sail callbacks in model order, then one nearest-hit event.
    pub fn shot(
        &mut self,
        asset: &ModelAsset,
        hits: &[SailImpact],
        owner: i32,
        character: i32,
    ) -> Result<()> {
        let total_weight: f32 = asset
            .meshes
            .iter()
            .filter_map(|m| m.sail.as_ref())
            .map(|p| p.weight as f32)
            .sum();
        let mut ordered = hits.iter().collect::<Vec<_>>();
        if character != -1
            && hits.iter().any(|h| {
                asset.meshes[h.mesh]
                    .sail
                    .as_ref()
                    .is_some_and(|p| p.origin.is_some())
            })
        {
            let mut attributes = self.attributes.borrow_mut();
            let character_root =
                attributes.create_path(self.root, &format!("characters.{character}"))?;
            attributes.set_attribute(character_root, "index", Some(&character.to_string()))?;
            let sails = attributes.create_path(character_root, "ship.sails")?;
            let max_holes: u32 = asset
                .meshes
                .iter()
                .filter_map(|m| m.sail.as_ref())
                .map(|p| p.section_count())
                .sum();
            attributes.set_value(sails, Some(&max_holes.to_string()))?;
        }
        ordered.sort_by_key(|h| h.mesh);
        for hit in ordered {
            let patch = asset.meshes[hit.mesh]
                .sail
                .as_ref()
                .expect("sail trace mesh");
            // The original suppresses DoSailHole without a valid character.
            if character != -1
                && let Some(origin) = &patch.origin
            {
                // SAIL::AddSailLabel uses atoi(&group_name[5]).
                let group = original_group(&origin.group);
                let mut message = Message::new(
                    "llssllllf",
                    vec![
                        Value::Long(owner),
                        Value::Long(character),
                        Value::String("*".into()),
                        Value::String(origin.node.clone()),
                        Value::Long(group),
                        Value::Long(hit.hole_mask.count_ones() as i32),
                        Value::Long(i32::from(hit.hole_mask)),
                        Value::Long(patch.section_count() as i32),
                        Value::Float(patch.weight as f32 / total_weight),
                    ],
                )?;
                self.dispatcher.emit("DoSailHole", &mut message)?;
            }
        }
        if let Some(hit) = hits.iter().min_by(|a, b| a.fraction.total_cmp(&b.fraction)) {
            let [x, y, z] = hit.position;
            let mut message = Message::new(
                "lfffl",
                vec![
                    Value::Long(character),
                    Value::Float(x),
                    Value::Float(y),
                    Value::Float(z),
                    Value::Long(owner),
                ],
            )?;
            self.dispatcher.emit("ShpSailDamage", &mut message)?;
        }
        Ok(())
    }
    pub fn write(&self, path: &Path, asset: &ModelAsset, character: i32) -> Result<()> {
        let mut restored = asset.ship_state.clone();
        restored.sail_holes.clear();
        let restored_count = self.restore_sail_damage(asset, character, &mut restored)?;
        let records = self.records.borrow();
        let attributes = self.attributes.borrow();
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "observer_entity":self.observer.raw(),"script_vm":false,"total_events":records.total,
                "retained_events":records.recent.len(),"events":records.recent,
                "attribute_notifications":records.attribute_notifications,
                "attributes":attribute_tree(&attributes,self.root),
                "attribute_restored_sails":restored_count,"attribute_sail_masks":restored.sail_holes,
            }))?,
        )?;
        Ok(())
    }
    /// The native 0-key repair must clear persistent damage as well as its mesh state.
    pub fn clear_sail_damage(&mut self, character: i32) -> Result<()> {
        let mut attributes = self.attributes.borrow_mut();
        if let Some(sails) =
            attributes.find_path(self.root, &format!("characters.{character}.ship.sails"))?
        {
            attributes.delete_class(sails, sails)?;
        }
        Ok(())
    }
    /// SetAllSails/FindSailFromData: exact node name, numeric group, hd's 10/12
    /// low bits. The native observer supplies the tree; no scripts execute.
    pub fn restore_sail_damage(
        &self,
        asset: &ModelAsset,
        character: i32,
        state: &mut ShipState,
    ) -> Result<usize> {
        let attributes = self.attributes.borrow();
        let Some(sails) =
            attributes.lookup_path(self.root, &format!("characters.{character}.ship.sails"))?
        else {
            return Ok(0);
        };
        let mut restored = 0;
        for node in attributes.get(sails).expect("live sails").children() {
            let node_name = attributes.name(*node).expect("live node");
            for group in attributes.get(*node).expect("live node").children() {
                let Some(number) = decimal_group(attributes.name(*group).expect("live group"))
                else {
                    continue;
                };
                let Some((mesh, _)) = asset.meshes.iter().enumerate().find(|(_, m)| {
                    m.sail
                        .as_ref()
                        .and_then(|p| p.origin.as_ref())
                        .is_some_and(|o| o.node == node_name && original_group(&o.group) == number)
                }) else {
                    continue;
                };
                asset.set_sail_holes(state, mesh, attributes.as_dword(*group, Some("hd"), 0))?;
                restored += 1;
            }
        }
        Ok(restored)
    }
}

fn original_group(group: &str) -> i32 {
    decimal_group(group.get(5..).unwrap_or("")).unwrap_or(0)
}
fn decimal_group(group: &str) -> Option<i32> {
    let suffix = group.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let length = suffix
        .bytes()
        .enumerate()
        .take_while(|(i, b)| b.is_ascii_digit() || (*i == 0 && matches!(*b, b'+' | b'-')))
        .count();
    suffix[..length].parse().ok()
}
fn attribute_tree(store: &AttributeStore, id: AttributeId) -> Json {
    let node = store.get(id).expect("live attribute report root");
    json!({"name":store.name(id),"value":node.value(),"children":node.children().iter().map(|id|attribute_tree(store,*id)).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shot_emits_original_payloads_even_when_damage_was_already_present() -> Result<()> {
        let mut asset = ModelAsset::ship_demo()?;
        let mesh = asset.meshes.iter().position(|m| m.sail.is_some()).unwrap();
        asset.meshes[mesh].sail.as_mut().unwrap().origin = Some(storm_render::model::SailOrigin {
            group: "saild12".into(),
            node: "rey_a2".into(),
            parent: "mast2".into(),
        });
        let hit = SailImpact {
            mesh,
            section: 0,
            fraction: 0.5,
            uv: [0.2, 0.3],
            position: [1., 2., 3.],
            new_damage: false,
            hole_mask: 5,
        };
        let mut events = DamageEvents::new()?;
        events.shot(&asset, &[hit], 7, 19)?;
        let records = events.records.borrow();
        assert_eq!(records.total, 2);
        let hole = &records.recent[0];
        assert_eq!(hole["name"], "DoSailHole");
        assert_eq!(hole["owner"], 7);
        assert_eq!(hole["character"], 19);
        assert_eq!(hole["node"], "rey_a2");
        assert_eq!(hole["group"], 12);
        assert_eq!(hole["hole_count"], 2);
        assert_eq!(hole["hole_mask"], 5);
        assert_eq!(records.recent[1]["position"], json!([1., 2., 3.]));
        assert_eq!(records.attribute_notifications, 1);
        drop(records);
        events.shot(&asset, &[hit], -1, -1)?;
        assert_eq!(events.records.borrow().total, 3);
        events.shot(&asset, &[], 7, 19)?;
        assert_eq!(events.records.borrow().total, 3);
        Ok(())
    }
    #[test]
    fn nearest_damage_event_follows_all_sail_callbacks() -> Result<()> {
        let mut asset = ModelAsset::ship_demo()?;
        let meshes = asset
            .meshes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.sail.as_ref().map(|_| i))
            .collect::<Vec<_>>();
        for (group, &mesh) in meshes.iter().enumerate() {
            asset.meshes[mesh].sail.as_mut().unwrap().origin =
                Some(storm_render::model::SailOrigin {
                    group: format!("saild{group}"),
                    node: format!("rey{group}"),
                    parent: "mast1".into(),
                });
        }
        let far = SailImpact {
            mesh: meshes[0],
            section: 0,
            fraction: 0.8,
            uv: [0., 0.],
            position: [8., 0., 0.],
            new_damage: true,
            hole_mask: 1,
        };
        let near = SailImpact {
            mesh: meshes[1],
            fraction: 0.2,
            position: [2., 0., 0.],
            ..far
        };
        let mut events = DamageEvents::new()?;
        events.shot(&asset, &[near, far], 1, 19)?;
        {
            let records = events.records.borrow();
            assert_eq!(records.total, 3);
            assert_eq!(records.recent[0]["node"], "rey0");
            assert_eq!(records.recent[1]["node"], "rey1");
            assert_eq!(records.recent[2]["position"], json!([2., 0., 0.]));
        }
        for _ in 0..300 {
            events.shot(&asset, &[far, near], 1, -1)?;
        }
        let records = events.records.borrow();
        assert_eq!(records.total, 303);
        assert_eq!(records.recent.len(), 256);
        assert_eq!(records.recent[0]["position"], json!([2., 0., 0.]));
        Ok(())
    }
    #[test]
    fn original_hd_attributes_roundtrip_masks_and_mask_high_bits() -> Result<()> {
        let asset = ModelAsset::ship_demo()?;
        let mut state = asset.ship_state.clone();
        let mut events = DamageEvents::new()?;
        asset.sail_volley_with(
            &mut state,
            3,
            Default::default(),
            0.0,
            storm_sim::RigState::for_mode(storm_sim::SailMode::Full),
            |hits| events.shot(&asset, hits, 7, 19),
        )?;
        let mut restored = asset.ship_state.clone();
        assert_eq!(events.restore_sail_damage(&asset, 19, &mut restored)?, 2);
        assert_eq!(restored.sail_holes, state.sail_holes);
        let attributes = events.attributes.borrow();
        let character = attributes
            .lookup_path(events.root, "characters.19")?
            .unwrap();
        assert_eq!(attributes.as_dword(character, Some("index"), 0), 19);
        let sails = attributes.lookup_path(character, "ship.sails")?.unwrap();
        assert_eq!(attributes.as_dword(sails, None, 0), 22);
        let node = attributes.child_at(sails, 0).unwrap();
        let group = attributes.child_at(node, 0).unwrap();
        let hd = attributes.child(group, "hd").unwrap();
        let node_name = attributes.name(node).unwrap().to_owned();
        let number = decimal_group(attributes.name(group).unwrap()).unwrap();
        drop(attributes);
        events
            .attributes
            .borrow_mut()
            .set_value(hd, Some("4294967295"))?;
        events.restore_sail_damage(&asset, 19, &mut restored)?;
        let (mesh, patch) = asset
            .meshes
            .iter()
            .enumerate()
            .find_map(|(i, m)| {
                m.sail
                    .as_ref()
                    .filter(|p| {
                        p.origin.as_ref().is_some_and(|o| {
                            o.node == node_name && original_group(&o.group) == number
                        })
                    })
                    .map(|p| (i, p))
            })
            .unwrap();
        assert_eq!(
            u32::from(restored.sail_holes[&mesh]),
            (1 << patch.section_count()) - 1
        );
        assert_eq!(events.restore_sail_damage(&asset, -1, &mut restored)?, 0);
        Ok(())
    }
    #[test]
    fn repair_clears_only_the_target_characters_persistent_masks() -> Result<()> {
        let asset = ModelAsset::ship_demo()?;
        let mut state = asset.ship_state.clone();
        let mut events = DamageEvents::new()?;
        asset.sail_volley_with(
            &mut state,
            3,
            Default::default(),
            0.0,
            storm_sim::RigState::for_mode(storm_sim::SailMode::Full),
            |hits| {
                events.shot(&asset, hits, 7, 19)?;
                events.shot(&asset, hits, 9, 20)
            },
        )?;
        events.clear_sail_damage(19)?;
        let mut repaired = asset.ship_state.clone();
        assert_eq!(events.restore_sail_damage(&asset, 19, &mut repaired)?, 0);
        assert!(repaired.sail_holes.is_empty());
        assert_eq!(events.restore_sail_damage(&asset, 20, &mut repaired)?, 2);
        assert_eq!(repaired.sail_holes, state.sail_holes);
        assert_eq!(events.records.borrow().attribute_notifications, 12);
        Ok(())
    }
}
