//! The island locator message reads GM labels without loading textures.
use crate::{GameDirectory, NativeHost};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use storm_core::{AttributeId, AttributeStore, Entity, Message, ScriptVariableKind};
use storm_formats::geometry::{Geometry, Label};

const LOCATE_ISLAND: i32 = 1504;
const MAX_MODEL_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
    host.entities
        .register_class("locator", move || Ok(Box::new(Locator(game.clone()))))?;
    Ok(())
}

struct Locator(GameDirectory);

impl Entity for Locator {
    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        attributes: &mut AttributeStore,
    ) -> Result<u64> {
        let code = message.long()?;
        if code != LOCATE_ISLAND {
            bail!("unsupported LOCATOR message {code}");
        }
        let island = message.script_variable()?;
        if matches!(
            island.kind()?,
            ScriptVariableKind::Object | ScriptVariableKind::AttributeReference
        ) {
            let root = island.attributes(attributes)?;
            self.locate_island(attributes, root)?;
        }
        Ok(0)
    }
}

impl Locator {
    fn locate_island(&self, store: &mut AttributeStore, root: AttributeId) -> Result<()> {
        let Some(model) = store.value(root, Some("locators")) else {
            return Ok(());
        };
        let folder = store
            .lookup_path(root, "filespath.models")?
            .and_then(|id| store.value(id, None))
            .unwrap_or_default();
        let file = if folder.is_empty() {
            format!("resource/models/{model}.gm")
        } else {
            format!("resource/models/{folder}/{model}.gm")
        };
        let Some(bytes) = self.0.read_optional(&file, MAX_MODEL_BYTES)? else {
            eprintln!("LOCATOR: island locator model not found: {file}");
            return Ok(());
        };
        let geometry =
            Geometry::read(&bytes).with_context(|| format!("decode LOCATOR model {file}"))?;
        write_island_labels(store, root, &geometry.labels)
    }
}

fn write_island_labels(
    store: &mut AttributeStore,
    root: AttributeId,
    labels: &[Label],
) -> Result<()> {
    let mut groups: BTreeMap<String, Vec<&Label>> = BTreeMap::new();
    for label in labels {
        groups
            .entry(label.group.to_ascii_lowercase())
            .or_default()
            .push(label);
    }

    if let Some(reload) = store.child(root, "reload")
        && let Some(labels) = groups.get("reload")
    {
        let ports = children(store, reload);
        for label in labels {
            for &port in &ports {
                if store
                    .value(port, Some("name"))
                    .is_some_and(|name| name.eq_ignore_ascii_case(&label.name))
                {
                    write_position(store, port, label, true)?;
                    // The reload label name is interned in the GM name table, even
                    // when there are no second-level ship berths with that name.
                    let berths = store.create_path(port, "ships")?;
                    if let Some(labels) = groups.get(&label.name.to_ascii_lowercase()) {
                        for (index, label) in labels.iter().enumerate() {
                            let berth = store.create_path(berths, &format!("l{index}"))?;
                            write_position(store, berth, label, true)?;
                        }
                    }
                }
            }
        }
    }

    for (name, append, heading) in [
        ("Quest_ships", false, true),
        ("net_deathmatch", true, true),
        ("net_team", true, true),
        ("net_convoy", true, true),
        ("net_fort", true, true),
        ("fire", false, false),
    ] {
        if let Some(labels) = groups.get(&name.to_ascii_lowercase()) {
            write_group(store, root, name, labels, append, heading)?;
        }
    }
    if let Some(load_group) = store.child(root, "LoadGroup") {
        for entry in children(store, load_group) {
            let Some(name) = store.value(entry, None).map(str::to_owned) else {
                continue;
            };
            if let Some(labels) = groups.get(&name.to_ascii_lowercase()) {
                write_group(store, root, &name, labels, false, true)?;
            }
        }
    }
    Ok(())
}

fn children(store: &AttributeStore, parent: AttributeId) -> Vec<AttributeId> {
    store
        .get(parent)
        .expect("existing parent")
        .children()
        .to_vec()
}

fn write_group(
    store: &mut AttributeStore,
    root: AttributeId,
    name: &str,
    labels: &[&Label],
    append: bool,
    heading: bool,
) -> Result<()> {
    let group = match if append {
        None
    } else {
        store.lookup_path(root, name)?
    } {
        Some(id) => id,
        None => store.create_attribute(root, name, Some(""))?,
    };
    for label in labels {
        let node = store.create_path(group, &label.name)?;
        write_position(store, node, label, heading)?;
    }
    Ok(())
}

fn write_position(
    store: &mut AttributeStore,
    id: AttributeId,
    label: &Label,
    heading: bool,
) -> Result<()> {
    for (key, value) in ["x", "y", "z"]
        .into_iter()
        .zip(label.matrix[12..15].iter().copied())
    {
        store.set_float(id, Some(key), value)?;
    }
    if heading {
        store.set_float(id, Some("ay"), label.matrix[8].atan2(label.matrix[10]))?;
    }
    Ok(())
}
