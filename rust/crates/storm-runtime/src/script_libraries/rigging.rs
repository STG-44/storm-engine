use crate::host::NativeFunction;
use anyhow::{Result, bail, ensure};
use storm_core::{AttributeId, AttributeStore, format_float};
use storm_render::model::original_sail_speed;
use storm_script::{MAX_STRING, Value};

pub(crate) const RIGGING_LIBRARY: &str = "script_rigging_files";

pub(crate) fn functions() -> [(&'static str, NativeFunction); 3] {
    [
        ("getassembledstring", NativeFunction::new(assembled_string)),
        (
            "funcgetsailspeed",
            NativeFunction::with_world(|args, _, entities| {
                let [Value::Int(holes), Value::Int(maximum), Value::Float(power)] = args else {
                    bail!("funcGetSailSpeed expects two integer counts and float sail power");
                };
                ensure!(
                    entities.first_id("sail").is_none(),
                    "native SAIL hole dependence is not connected"
                );
                // g_fSailHoleDepend starts at 1 until a native SAIL reads rigging.ini.
                Ok(Value::Float(
                    power - original_sail_speed(*holes, *maximum, *power, 1.0),
                ))
            }),
        ),
        (
            "shipsailstate",
            NativeFunction::with_world(|args, _, entities| {
                let [Value::Int(_)] = args else {
                    bail!("ShipSailState expects an integer character index");
                };
                ensure!(
                    entities.first_id("sail").is_none(),
                    "native SAIL state is not connected"
                );
                Ok(Value::Int(0))
            }),
        ),
    ]
}

fn assembled_string(args: &[Value]) -> Result<Value> {
    let [Value::String(template), data] = args else {
        bail!("GetAssembledString expects a template and attribute reference");
    };
    let data = data.resolved()?;
    if matches!(data, Value::AReference(None)) {
        return Ok(Value::String(String::new()));
    }
    let text = data
        .attributes()?
        .inspect(|store, root| expand(template, store, root))??;
    Ok(Value::String(text))
}

fn expand(template: &str, store: &AttributeStore, root: AttributeId) -> Result<String> {
    let mut result = String::new();
    for (index, segment) in template.split('#').enumerate() {
        if index % 2 == 0 {
            append(&mut result, segment)?;
            continue;
        }
        let (path, precision) = match segment.as_bytes() {
            [b'f', b'.', digit @ b'0'..=b'9', ..] => (&segment[3..], (digit - b'0') as usize),
            [b'f', b'.', ..] => continue,
            [b's' | b'd' | b'f', _, ..] => (&segment[1..], 6),
            _ => continue,
        };
        let Some(id) = store.lookup_path(root, path)? else {
            continue;
        };
        let Some(text) = store.value(id, None) else {
            continue;
        };
        match segment.as_bytes()[0] {
            b's' => append(&mut result, text)?,
            b'd' => append(
                &mut result,
                &(store.as_dword(id, None, 0) as i32).to_string(),
            )?,
            b'f' => {
                let value = store.as_float(id, None, 0.0);
                let formatted = if value.is_finite() {
                    format!("{:.precision$}", f64::from(value))
                } else {
                    format_float(value)
                };
                append(&mut result, &formatted)?;
            }
            _ => unreachable!(),
        }
    }
    Ok(result)
}

fn append(output: &mut String, text: &str) -> Result<()> {
    ensure!(
        output.len() + text.len() <= MAX_STRING,
        "script string limit"
    );
    output.push_str(text);
    Ok(())
}
