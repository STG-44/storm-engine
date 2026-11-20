//! Native location and character lookups from location_script_lib.cpp.
use crate::{host::NativeFunction, interface::Interfaces};
use anyhow::{Result, bail, ensure};
use storm_script::{Array, Value};

pub(crate) const LOCATION_LIBRARY: &str = "scriptlocationlibrary";

#[derive(Default)]
struct CacheEntry {
    id: String,
    index: Option<i32>,
    uses: u8,
}

fn matches_id(array: &Array, index: i32, id: &str) -> Result<Option<bool>> {
    let Ok(value) = array.element_value(index) else {
        return Ok(None);
    };
    let root = match &value {
        Value::Object(object) => object.root(),
        Value::AReference(Some(root)) => root.clone(),
        Value::AReference(None) => return Ok(None),
        _ => return Ok(Some(false)),
    };
    // No id is a mismatch, while a missing attribute root invalidates a cache entry.
    Ok(Some(root.inspect(|store, root| {
        store
            .value(root, Some("id"))
            .is_some_and(|value| value.eq_ignore_ascii_case(id))
    })?))
}

fn find<const N: usize>(cache: &mut [CacheEntry; N], args: &[Value]) -> Result<Value> {
    let [reference @ Value::Reference(_), name] = args else {
        bail!("native location search expects an array reference and an ID");
    };
    let Value::Array(array) = reference.resolved()? else {
        bail!("native location search expects an array reference");
    };
    let Value::String(id) = name.resolved()? else {
        bail!("native location search ID requires string");
    };
    if id.is_empty() {
        return Ok(Value::Int(-1));
    }
    for entry in cache.iter_mut() {
        entry.uses = entry.uses.saturating_sub(1);
    }
    for entry in cache.iter_mut() {
        if let Some(index) = entry.index.filter(|_| entry.id.eq_ignore_ascii_case(&id)) {
            match matches_id(&array, index, &id)? {
                Some(true) => {
                    entry.uses += 1;
                    return Ok(Value::Int(index));
                }
                Some(false) => {}
                None => entry.index = None,
            }
        }
    }
    for index in 0..array.len() as i32 {
        if matches_id(&array, index, &id)? == Some(true) {
            let destination = cache
                .iter_mut()
                .min_by_key(|entry| (entry.index.is_some(), entry.index.map_or(0, |_| entry.uses)))
                .expect("location search cache is nonempty");
            *destination = CacheEntry {
                id,
                index: Some(index),
                uses: 16,
            };
            return Ok(Value::Int(index));
        }
    }
    Ok(Value::Int(-1))
}

pub(crate) fn functions(interfaces: Interfaces) -> [(&'static str, NativeFunction); 8] {
    let mut characters: [CacheEntry; 16] = std::array::from_fn(|_| CacheEntry::default());
    let mut locations: [CacheEntry; 8] = std::array::from_fn(|_| CacheEntry::default());
    let progress_start = interfaces.clone();
    let progress_update = interfaces.clone();
    let progress_picture = interfaces.clone();
    [
        (
            "nativefindcharacter",
            NativeFunction::new(move |args| find(&mut characters, args)),
        ),
        (
            "nativefindlocation",
            NativeFunction::new(move |args| find(&mut locations, args)),
        ),
        (
            "nativefindloadcharacter",
            NativeFunction::with_world(|args, world, entities| {
                ensure!(
                    args.is_empty(),
                    "NativeFindLoadCharacter expects no arguments"
                );
                let root = entities
                    .first_id("location")
                    .and_then(|id| entities.attribute_root(id));
                let index = match root {
                    Some(root) => {
                        world.read(|store| store.as_dword(root, Some("index"), u32::MAX) as i32)?
                    }
                    None => -1,
                };
                Ok(Value::Int(index))
            }),
        ),
        (
            "getnextlinestring",
            NativeFunction::new(|args| {
                ensure!(args.is_empty(), "GetNextLineString expects no arguments");
                Ok(Value::String("\r\n".into()))
            }),
        ),
        (
            "setreloadprogressbackimage",
            NativeFunction::new(move |args| {
                let [name] = args else {
                    bail!("SetReloadProgressBackImage expects one argument");
                };
                let Value::String(name) = name.resolved()? else {
                    bail!("SetReloadProgressBackImage expects a texture name");
                };
                progress_picture.set_loading_progress_foreground(name);
                Ok(Value::Void)
            }),
        ),
        (
            "reloadprogressstart",
            NativeFunction::new(move |args| {
                ensure!(args.is_empty(), "ReloadProgressStart expects no arguments");
                progress_start.start_loading_progress()?;
                Ok(Value::Void)
            }),
        ),
        (
            "reloadprogressupdate",
            NativeFunction::new(move |args| {
                ensure!(args.is_empty(), "ReloadProgressUpdate expects no arguments");
                progress_update.update_loading_progress()?;
                Ok(Value::Void)
            }),
        ),
        (
            "reloadprogressend",
            NativeFunction::new(move |args| {
                ensure!(args.is_empty(), "ReloadProgressEnd expects no arguments");
                interfaces.end_loading_progress();
                Ok(Value::Void)
            }),
        ),
    ]
}
