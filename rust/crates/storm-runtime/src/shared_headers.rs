//! Original engine-owned script headers, independently of game PROGRAM paths.
use anyhow::{Result, bail};

pub(crate) fn source(name: &str) -> Result<&'static str> {
    macro_rules! header {
        ($name:literal) => {
            include_str!(concat!(
                "../../../../src/libs/shared_headers/include/shared/",
                $name
            ))
        };
    }
    Ok(match name {
        "layers.h" => header!("layers.h"),
        "events.h" => header!("events.h"),
        "controls.h" => header!("controls.h"),
        "sound.h" => header!("sound.h"),
        "messages.h" => header!("messages.h"),
        "mast_msg.h" => header!("mast_msg.h"),
        "sail_msg.h" => header!("sail_msg.h"),
        "bimanager/messages.h" => header!("bimanager/messages.h"),
        "battle_interface/msg_control.h" => header!("battle_interface/msg_control.h"),
        "battle_interface/log_msg.h" => header!("battle_interface/log_msg.h"),
        "sea_ai/sea_people.h" => header!("sea_ai/sea_people.h"),
        "sea_ai/script_defines.h" => header!("sea_ai/script_defines.h"),
        "interface/messages.h" => header!("interface/messages.h"),
        _ => bail!("reserved Storm header is not bundled: {name}"),
    })
}
