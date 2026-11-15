//! The original SteamApi implementation used by builds without Steam SDK.
use crate::host::NativeFunction;
use anyhow::ensure;
use storm_script::Value;

pub(crate) const STEAM_LIBRARY: &str = "steamapiscriptlib";

#[derive(Clone, Copy)]
enum Arguments {
    None,
    Integer,
    String,
    Stat,
}

pub(crate) fn offline_functions() -> [(&'static str, NativeFunction); 12] {
    use Arguments::{Integer, None, Stat, String};
    [
        ("getsteamenabled", None, 0),
        ("getdlcenabled", Integer, 1),
        ("getdlccount", None, 0),
        ("getdlcdata", Integer, 0),
        ("dlcstartoverlay", Integer, 0),
        ("setachievement", String, 0),
        ("getachievement", String, 0),
        ("setstat", Stat, 0),
        ("getstat", String, 0),
        ("storestats", None, 0),
        ("clearachievement", String, 0),
        ("resetstats", Integer, 0),
    ]
    .map(|(name, signature, result)| {
        let function: NativeFunction = NativeFunction::new(move |args| {
            let valid = match (signature, args) {
                (None, []) | (String, [Value::String(_)]) => true,
                (Integer, [value]) | (Stat, [Value::String(_), value]) => {
                    matches!(value.resolved()?, Value::Int(_))
                }
                _ => false,
            };
            ensure!(valid, "{name} has invalid Steam script arguments");
            Ok(Value::Int(result))
        });
        (name, function)
    })
}
