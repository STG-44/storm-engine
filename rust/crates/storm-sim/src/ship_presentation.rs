//! Living SHIP::Execute frame boundary: pre-Move pose and post-Move turn heel.
use crate::{
    ship_motion::{MovementPort, ShipMotion},
    ship_placement::RootPose,
};
use anyhow::{Result, ensure};
use storm_core::{AttributeId, AttributeStore};

/// Native model presentation lags movement by one Execute call. The additive
/// turn heel is presentation-only and must never feed the next rocking state.
pub fn advance_living(
    body: &mut ShipMotion,
    delta_ms: u32,
    port: &mut impl MovementPort,
) -> Result<RootPose> {
    ensure!(
        !body.dead,
        "living ship presentation cannot execute a dead body"
    );
    let mut pose = RootPose {
        position: body.state.position,
        angles: body.state.angles,
    };
    body.advance(delta_ms, false, port)?;
    // advance has checked finiteness; clamp also preserves native signed zeros.
    let heel = body.state.rotation[1].clamp(-0.25, 0.25);
    pose.angles[2] += heel;
    Ok(pose)
}

/// Publish the post-Move state used by original scripts. This deliberately does
/// not publish immersion-adjusted model height, presentation lag or turn heel.
/// Numeric/handle validation precedes writes; allocation failures can retain
/// earlier attribute writes, as with other checked attribute operations.
pub fn publish_character(
    store: &mut AttributeStore,
    character: AttributeId,
    body: &ShipMotion,
) -> Result<()> {
    ensure!(
        store.get(character).is_some(),
        "stale or foreign presentation character"
    );
    let state = body.state;
    let values = [
        [state.speed[0], state.rotation[1], state.speed[2]],
        [
            state.position[0] + body.offsets[0],
            state.position[1],
            state.position[2] + body.offsets[1],
        ],
        state.angles,
    ];
    ensure!(
        values.into_iter().flatten().all(f32::is_finite),
        "non-finite script ship state"
    );
    for (path, vector) in ["ship.speed", "ship.pos", "ship.ang"]
        .into_iter()
        .zip(values)
    {
        let node = store.create_path(character, path)?;
        for (name, value) in ["x", "y", "z"].into_iter().zip(vector) {
            store.set_float(node, Some(name), value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
