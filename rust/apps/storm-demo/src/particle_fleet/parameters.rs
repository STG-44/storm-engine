//! Bounded external attributes for the native parameter/character readers.
use anyhow::{Context, Result, ensure};
use serde_json::Value as Json;
use std::{io::Read, path::Path};
use storm_core::{AttributeId, AttributeStore};
const LIMIT: usize = 64 * 1024;

pub(super) fn read(path: &Path) -> Result<Json> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .with_context(|| format!("open ship parameters {}", path.display()))?
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    parse(&bytes)
}
fn entries(config: &Json) -> Result<Vec<(bool, String, Option<String>)>> {
    let object = config
        .as_object()
        .context("ship parameters must be an object")?;
    ensure!(
        object.len() <= 2
            && object
                .keys()
                .all(|k| matches!(k.as_str(), "ship_type" | "character")),
        "select ship_type and/or character attributes"
    );
    let mut result = Vec::new();
    for (section, values) in object {
        let values = values
            .as_object()
            .context("attribute section must be an object")?;
        ensure!(values.len() <= 128, "too many ship attributes");
        for (name, value) in values {
            ensure!(
                !name.is_empty() && name.len() <= 1023 && !name.contains('\0'),
                "invalid ship attribute name"
            );
            if section == "character" {
                ensure!(
                    name.split('.').all(|s| !s.is_empty()) && name.split('.').count() <= 256,
                    "invalid character attribute path"
                );
            }
            let text = match value {
                Json::String(s) => Some(s.clone()),
                Json::Number(n) => Some(n.to_string()),
                Json::Null => None,
                _ => anyhow::bail!("attribute values must be strings, numbers or null"),
            };
            ensure!(
                text.as_ref().is_none_or(|s| s.len() <= 4096),
                "ship attribute text too long"
            );
            result.push((section == "character", name.clone(), text));
        }
    }
    Ok(result)
}
fn parse(bytes: &[u8]) -> Result<Json> {
    ensure!(bytes.len() <= LIMIT, "ship parameter file exceeds 64 KiB");
    let config: Json = serde_json::from_slice(bytes)?;
    entries(&config)?;
    Ok(config)
}
pub(super) fn apply(
    store: &mut AttributeStore,
    ship_type: AttributeId,
    character: AttributeId,
    config: &Json,
) -> Result<()> {
    ensure!(
        store.get(ship_type).is_some() && store.get(character).is_some(),
        "invalid parameter input bindings"
    );
    for (is_character, name, text) in entries(config)? {
        if is_character {
            let id = store.create_path(character, &name)?;
            store.set_value(id, text.as_deref())?;
        } else {
            store.set_attribute(ship_type, &name, text.as_deref())?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_attributes_keep_type_and_character_values_separate() {
        let config=parse(br#"{"ship_type":{"SpeedRate":" 18 knots","Class":-1,"InertiaAcceleration.X":9,"WaterLine":null},"character":{"Ship.MaxSpeedZ":10,"Ship.MaxSpeedY":"0.06"}}"#).unwrap();
        let mut store = AttributeStore::default();
        let ship = store.create_root().unwrap();
        let character = store.create_root().unwrap();
        apply(&mut store, ship, character, &config).unwrap();
        let p = storm_sim::ship_parameters::ShipParameters::load(&store, ship).unwrap();
        assert_eq!(p.speed_rate, 18.0);
        assert_eq!(p.class, -1);
        assert_eq!(p.waterline, 0.0);
        assert_eq!(p.inertia_accel, [0.0; 3]);
        let limits =
            storm_sim::ship_motion::FlatMovement::from_character(&store, character).unwrap();
        assert_eq!(limits.max_speed_z, 10.0);
        assert_eq!(limits.max_speed_y, 0.06);
    }
    #[test]
    fn malformed_or_oversized_configuration_is_rejected_before_attribute_writes() {
        for bytes in [
            b"[]".as_slice(),
            br#"{"other":{}}"#,
            br#"{"ship_type":{"Weight":true}}"#,
            br#"{"character":{"Ship..MaxSpeedZ":3}}"#,
            br#"{"ship_type":[]}"#,
        ] {
            assert!(parse(bytes).is_err());
        }
        assert!(parse(&vec![b' '; LIMIT + 1]).is_err());
        let mut store = AttributeStore::default();
        let ship = store.create_root().unwrap();
        let character = store.create_root().unwrap();
        let len = store.len();
        assert!(
            apply(
                &mut store,
                ship,
                character,
                &serde_json::json!({"ship_type":{"Weight":[]}})
            )
            .is_err()
        );
        assert_eq!(store.len(), len);
        assert!(
            apply(
                &mut AttributeStore::default(),
                ship,
                character,
                &serde_json::json!({})
            )
            .is_err()
        );
    }
}
