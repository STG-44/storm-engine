//! Original cannon/sail payloads executed by bounded diagnostic source handlers.
mod script;
use anyhow::Result;
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::VecDeque, path::Path, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, Message, Value};
use storm_render::model::{ModelAsset, SailImpact, ShipState};
use storm_script::{Limits, Runtime};

#[derive(Default)]
struct Records {
    total: u64,
    recent: VecDeque<Json>,
    attribute_notifications: u64,
    script_damage_calls: u64,
    script_last_damage: Option<Json>,
}
struct Observer(Rc<RefCell<Records>>);
impl Entity for Observer {
    fn attribute_changed(
        &mut self,
        attributes: &AttributeStore,
        changed: AttributeId,
    ) -> Result<u32> {
        anyhow::ensure!(
            attributes.name(changed) == Some("hd"),
            "unexpected damage attribute notification"
        );
        self.0.borrow_mut().attribute_notifications += 1;
        Ok(0)
    }
}

pub struct DamageEvents {
    runtime: Runtime,
    records: Rc<RefCell<Records>>,
    host: script::DamageHost,
}
impl DamageEvents {
    pub fn new() -> Result<Self> {
        let records = Rc::new(RefCell::new(Records::default()));
        let mut host = script::DamageHost::new(records.clone())?;
        let mut runtime = Runtime::compile(include_str!("../fixtures/sail_events.c"))?;
        runtime.call("InitSailEvents", &[], &mut host, Limits::default())?;
        Ok(Self {
            runtime,
            records,
            host,
        })
    }
    fn emit(&mut self, name: &str, message: &mut Message) -> Result<()> {
        self.runtime
            .emit(name, message, &mut self.host, Limits::default())?;
        message.move_to_start();
        let record = match name {
            "DoSailHole" => json!({"name":name,"format":message.format(),"owner":message.long()?,
                "character":message.long()?,"model":message.string()?.to_owned(),"node":message.string()?.to_owned(),
                "group":message.long()?,"hole_count":message.long()?,"hole_mask":message.long()?,
                "max_holes":message.long()?,"sail_weight_fraction":message.float()?}),
            "ShpSailDamage" => {
                json!({"name":name,"format":message.format(),"character":message.long()?,
                "position":[message.float()?,message.float()?,message.float()?],"owner":message.long()?})
            }
            _ => unreachable!(),
        };
        let mut records = self.records.borrow_mut();
        records.total += 1;
        if records.recent.len() == 256 {
            records.recent.pop_front();
        }
        records.recent.push_back(record);
        Ok(())
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
            let character_root = self
                .host
                .root()?
                .create_path(&format!("characters.{character}"))?;
            character_root
                .create_path("index")?
                .set_value(&character.to_string())?;
            let sails = character_root.create_path("ship.sails")?;
            let max_holes: u32 = asset
                .meshes
                .iter()
                .filter_map(|m| m.sail.as_ref())
                .map(|p| p.section_count())
                .sum();
            sails.set_value(&max_holes.to_string())?;
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
                self.emit("DoSailHole", &mut message)?;
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
            self.emit("ShpSailDamage", &mut message)?;
        }
        Ok(())
    }
    pub fn write(&self, path: &Path, asset: &ModelAsset, character: i32) -> Result<()> {
        let mut restored = asset.ship_state.clone();
        restored.sail_holes.clear();
        let restored_count = self.restore_sail_damage(asset, character, &mut restored)?;
        let records = self.records.borrow();
        let attributes = self.host.root()?.inspect(attribute_tree)?;
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "observer_entity":self.host.observer.raw(),"script_vm":true,"original_game_scripts":false,
                "script_handlers":self.runtime.completed_handlers(),"script_damage_calls":records.script_damage_calls,
                "script_last_damage":records.script_last_damage,"total_events":records.total,
                "retained_events":records.recent.len(),"events":records.recent,
                "attribute_notifications":records.attribute_notifications,
                "attributes":attributes,
                "attribute_restored_sails":restored_count,"attribute_sail_masks":restored.sail_holes,
            }))?,
        )?;
        Ok(())
    }
    /// The native 0-key repair must clear persistent damage as well as its mesh state.
    pub fn clear_sail_damage(&mut self, character: i32) -> Result<()> {
        self.host
            .root()?
            .delete_path(&format!("characters.{character}.ship.sails"))?;
        Ok(())
    }
    /// SetAllSails/FindSailFromData: exact node name, numeric group, hd's 10/12
    /// low bits. Script handlers populate the tree; restoration reads the same owned World.
    pub fn restore_sail_damage(
        &self,
        asset: &ModelAsset,
        character: i32,
        state: &mut ShipState,
    ) -> Result<usize> {
        self.host
            .root()?
            .inspect(|attributes, root| -> Result<usize> {
                let Some(sails) =
                    attributes.lookup_path(root, &format!("characters.{character}.ship.sails"))?
                else {
                    return Ok(0);
                };
                let mut restored = 0;
                for node in attributes.get(sails).expect("live sails").children() {
                    let node_name = attributes.name(*node).expect("live node");
                    for group in attributes.get(*node).expect("live node").children() {
                        let Some(number) =
                            decimal_group(attributes.name(*group).expect("live group"))
                        else {
                            continue;
                        };
                        let Some((mesh, _)) = asset.meshes.iter().enumerate().find(|(_, m)| {
                            m.sail
                                .as_ref()
                                .and_then(|p| p.origin.as_ref())
                                .is_some_and(|o| {
                                    o.node == node_name && original_group(&o.group) == number
                                })
                        }) else {
                            continue;
                        };
                        asset.set_sail_holes(
                            state,
                            mesh,
                            attributes.as_dword(*group, Some("hd"), 0),
                        )?;
                        restored += 1;
                    }
                }
                Ok(restored)
            })?
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
        assert_eq!(records.script_damage_calls, 1);
        assert_eq!(
            records.script_last_damage,
            Some(json!({"character":19,"position":[1.,2.,3.],"owner":7}))
        );
        assert_eq!(events.runtime.completed_handlers(), 2);
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
    fn script_assignment_drives_persistence_instead_of_the_observer() -> Result<()> {
        let asset = ModelAsset::ship_demo()?;
        let mesh = asset.meshes.iter().position(|m| m.sail.is_some()).unwrap();
        let mut events = DamageEvents::new()?;
        let source = include_str!("../fixtures/sail_events.c").replace(
            "ship.ship.sails.(node).(group).hd = mask;",
            "ship.ship.sails.(node).(group).hd = 3;",
        );
        events.runtime = Runtime::compile(&source)?;
        events
            .runtime
            .call("InitSailEvents", &[], &mut events.host, Limits::default())?;
        events.shot(
            &asset,
            &[SailImpact {
                mesh,
                section: 0,
                fraction: 0.5,
                uv: [0.; 2],
                position: [1., 2., 3.],
                new_damage: true,
                hole_mask: 5,
            }],
            7,
            19,
        )?;
        let mut restored = asset.ship_state.clone();
        assert_eq!(events.restore_sail_damage(&asset, 19, &mut restored)?, 1);
        assert_eq!(restored.sail_holes[&mesh], 3);
        assert_eq!(events.records.borrow().recent[0]["hole_mask"], 5);
        assert_eq!(events.records.borrow().attribute_notifications, 1);
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
        let (node_name, number) =
            events
                .host
                .root()?
                .inspect(|attributes, root| -> Result<_> {
                    let character = attributes.lookup_path(root, "characters.19")?.unwrap();
                    assert_eq!(attributes.as_dword(character, Some("index"), 0), 19);
                    let sails = attributes.lookup_path(character, "ship.sails")?.unwrap();
                    assert_eq!(attributes.as_dword(sails, None, 0), 22);
                    let node = attributes.child_at(sails, 0).unwrap();
                    let group = attributes.child_at(node, 0).unwrap();
                    Ok((
                        attributes.name(node).unwrap().to_owned(),
                        decimal_group(attributes.name(group).unwrap()).unwrap(),
                    ))
                })??;
        events
            .host
            .root()?
            .create_path(&format!("characters.19.ship.sails.{node_name}.{number}.hd"))?
            .set_value("4294967295")?;
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
