//! Native observer for original cannon/sail event payloads. No game scripts run here.
use anyhow::{Result, bail};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::VecDeque, path::Path, rc::Rc};
use storm_core::{Entity, EntityId, EntityManager, EventDispatcher, Message, Value};
use storm_render::model::{ModelAsset, SailImpact};

#[derive(Default)]
struct Records {
    total: u64,
    recent: VecDeque<Json>,
}
struct Observer(Rc<RefCell<Records>>);
impl Entity for Observer {
    fn process_message(&mut self, m: &mut Message) -> Result<u64> {
        let record = match m.format() {
            "llssllllf" => {
                let owner = m.long()?;
                let character = m.long()?;
                let model = m.string()?.to_owned();
                let node = m.string()?.to_owned();
                json!({"name":"DoSailHole", "format":"llssllllf", "owner":owner,
                    "character":character,"model":model,"node":node,"group":m.long()?,
                    "hole_count":m.long()?,"hole_mask":m.long()?,"max_holes":m.long()?,"sail_weight_fraction":m.float()?})
            }
            "lfffl" => json!({"name":"ShpSailDamage", "format":"lfffl", "character":m.long()?,
                "position":[m.float()?,m.float()?,m.float()?],"owner":m.long()?}),
            _ => bail!("unsupported native damage observer message"),
        };
        let mut records = self.0.borrow_mut();
        records.total += 1;
        if records.recent.len() == 256 {
            records.recent.pop_front();
        }
        records.recent.push_back(record);
        Ok(0)
    }
}

pub struct DamageEvents {
    dispatcher: EventDispatcher,
    records: Rc<RefCell<Records>>,
    observer: EntityId,
}
impl DamageEvents {
    pub fn new() -> Result<Self> {
        let records = Rc::new(RefCell::new(Records::default()));
        let mut entities = EntityManager::default();
        let observer =
            entities.insert("preview_sail_damage_observer", Observer(records.clone()))?;
        let entities = Rc::new(RefCell::new(entities));
        let mut dispatcher = EventDispatcher::default();
        for name in ["DoSailHole", "ShpSailDamage"] {
            let entities = entities.clone();
            dispatcher.register(name, move |message| {
                entities.borrow_mut().send_message(observer, message)?;
                Ok(None)
            })?;
        }
        Ok(Self {
            dispatcher,
            records,
            observer,
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
    pub fn write(&self, path: &Path) -> Result<()> {
        let records = self.records.borrow();
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "observer_entity":self.observer.raw(),"script_vm":false,"total_events":records.total,
                "retained_events":records.recent.len(),"events":records.recent,
            }))?,
        )?;
        Ok(())
    }
}

fn original_group(group: &str) -> i32 {
    let suffix = group.get(5..).unwrap_or("").trim_start();
    let length = suffix
        .bytes()
        .enumerate()
        .take_while(|(i, b)| b.is_ascii_digit() || (*i == 0 && matches!(*b, b'+' | b'-')))
        .count();
    suffix[..length].parse().unwrap_or(0)
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
}
