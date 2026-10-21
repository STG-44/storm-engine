use anyhow::{Result, ensure};
use serde_json::{Value, json};
use storm_render::model::ModelAsset;
use storm_sim::{RigState, SailMode, SailTier};

pub fn report(asset: &ModelAsset, seconds: f32) -> Result<Value> {
    let sails = asset
        .meshes
        .iter()
        .enumerate().filter_map(|(i,m)| m.sail.as_ref().map(|p|(i,p)))
        .map(|(index,patch)| {
            let origin = patch.origin.as_ref();
            json!({ "mesh":index,"hole_mask":asset.ship_state.sail_holes.get(&index).copied().unwrap_or(0),"damage_sections":patch.section_count(),"intact_fraction":patch.intact_fraction(asset.ship_state.sail_holes.get(&index).copied().unwrap_or(0)), "group": origin.map(|o| &o.group), "node": origin.map(|o| &o.node),
            "parent": origin.map(|o| &o.parent), "tier": patch.tier.name(),
            "triangular": patch.is_triangle(), "original_size_weight": patch.weight,"damage_settings":{"speed_depend":patch.damage_settings.speed_depend,"triangle_flex":patch.damage_settings.triangle_flex,"quad_flex":patch.damage_settings.quad_flex},"damage_grid_vertices":patch.vertex_count() })
        })
        .collect::<Vec<_>>();
    let normal = asset
        .meshes
        .iter()
        .filter_map(|m| m.sail.as_ref())
        .filter(|p| p.tier == SailTier::Normal)
        .count();
    let layout = asset.rig_layout();
    let r = asset.rigging_stats;
    let flags=asset.meshes.iter().filter_map(|m| match &m.rigging {
        Some(storm_render::model::RigMesh::Flag(f))=>{
            let origin=f.origin.as_ref();
            let cell=match f.texture() {storm_sim::FlagTexture::Cell(c)=>json!([c.column,c.row]),storm_sim::FlagTexture::Hidden=>Value::Null};
            Some(json!({"node":origin.map(|o|&o.node),"group":origin.map(|o|&o.group),"special":origin.is_some_and(|o|o.special),"atlas":f.atlas_size(),"cell":cell,"visible":f.is_visible()}))
        },_=>None
    }).collect::<Vec<_>>();
    Ok(
        json!({ "sail_damage_texture":asset.sail_damage.as_ref().map(|m|&m.source),"geometry_files": asset.geometry_files, "sail_count": sails.len(),
        "normal_count": normal, "full_only_count": sails.len()-normal,
        "normal_weight": layout.normal_weight, "full_only_weight": layout.full_only_weight,"missing_sail_weight":layout.missing_weight,
        "normal_mode_fraction": RigState::for_mode(SailMode::Normal).fraction(layout), "sails": sails, "flags":flags,
        "ship":ship_frame(asset,seconds)?,
        "rigging": { "ropes":r.ropes,"shrouds":r.shrouds,"flags":r.flags,
            "unmatched_ropes":r.unmatched_ropes,"incomplete_shrouds":r.incomplete_shrouds,
            "incomplete_flags":r.incomplete_flags,"invalid_lengths":r.invalid_lengths,"duplicate_points":r.duplicate_points } }),
    )
}

pub fn ship_frame(asset: &ModelAsset, seconds: f32) -> Result<Value> {
    let state = &asset.ship_state;
    state.validate(&asset.ship)?;
    let total = asset
        .meshes
        .iter()
        .filter_map(|mesh| mesh.dynamic.as_ref())
        .filter(|d| d.particle())
        .try_fold(0usize, |sum, d| {
            Ok::<_, anyhow::Error>(sum + d.frame_vertex_count(state, seconds)?)
        })?;
    ensure!(
        total <= storm_render::model::MAX_PARTICLE_VERTICES,
        "particle report exceeds scene vertex budget"
    );
    let mut report = json!({"crew_count":asset.ship.crew_count,"crew_visible":state.crew_visible,"waypoint_counts":asset.ship.waypoint_counts,"waypoint_missing_links":asset.ship.waypoint_missing_links,"crew_source":asset.ship.crew_source,
        "masts":asset.ship.masts(),"hulls":asset.ship.hulls(),
        "fire_points":asset.ship.fires.iter().map(|p|json!({"node":p.node,"name":p.name,"position":p.position.to_array()})).collect::<Vec<_>>(),
        "sail_holes":state.sail_holes,"broken_masts":state.broken_masts,"broken_hulls":state.broken_hulls,"fire_started":state.fire_started,"seconds":seconds,
        "crew_positions":asset.meshes.iter().filter_map(|m|match &m.dynamic {
            Some(storm_render::model::DynamicMesh::Crew(c))=>{let(p,_)=c.route.sample(seconds+c.phase);Some([p.x,p.y,-p.z])},_=>None
        }).collect::<Vec<_>>()});
    let profiles=asset.meshes.iter().filter_map(|m| match &m.dynamic {Some(storm_render::model::DynamicMesh::Particles(p))=>p.style.as_ref().map(|s|json!({"source":s.source,"kind":if !s.preview {"message"}else if p.smoke {"smoke"}else{"fire"},"slots_per_locator":p.per_point,"maximum_lifetime":p.lifetime,"uv_frames":s.frames.len()})),_=>None}).collect::<Vec<_>>();
    if !profiles.is_empty() {
        report["xps_particle_styles"] = json!(profiles);
    }
    let mut births = vec![];
    let mut motions = vec![];
    let mut world = vec![];
    for (mesh_index, mesh) in asset.meshes.iter().enumerate() {
        let Some(storm_render::model::DynamicMesh::Particles(patch)) = &mesh.dynamic else {
            continue;
        };
        let Some(style) = &patch.style else {
            continue;
        };
        for locator in 0..patch.points.len() {
            let activations: Vec<_> = if state.native_fire.is_some() {
                patch
                    .emissions(state, locator)
                    .filter(|e| {
                        seconds >= e.started
                            && e.stopped.is_none_or(|stop| seconds < stop + patch.lifetime)
                    })
                    .map(|e| (e.started, e.stopped, e.instance))
                    .collect()
            } else {
                state
                    .fire_started
                    .filter(|_| patch.manual_preview())
                    .map(|s| (s, None, 0))
                    .into_iter()
                    .collect()
            };
            for (start, stop, instance) in activations {
                let stream = storm_render::model::ParticleStream { locator, instance };
                let tag = |mut row: Value| {
                    if instance != 0 {
                        row["instance"] = json!(instance);
                    }
                    row
                };
                match style.instance_snapshot(seconds, start, stop, stream, None) {
                    Ok((_, b, m)) => {
                        births.push(tag(json!({"mesh":mesh_index,"locator":locator,"source":style.source,"created":b.created,"living":b.living,"emitter_clock_bits":b.clock_bits,"remainder_bits":b.remainder_bits})));
                        motions.push(tag(json!({"mesh":mesh_index,"locator":locator,"source":style.source,"living":m.living,"moving":m.moving,"spinning":m.spinning,"position_bounds":m.position_bounds,"angle_bounds":m.angle_bounds,"maximum_speed":m.maximum_speed})));
                    }
                    Err(error) => {
                        let row = tag(
                            json!({"mesh":mesh_index,"locator":locator,"error":error.to_string()}),
                        );
                        births.push(row.clone());
                        motions.push(row);
                    }
                }
                if let Some(history) = &state.particle_history {
                    world.push(tag(match style.instance_snapshot(seconds,start,stop,stream,Some(storm_render::model::WorldEmitter {history:history.clone(),point:patch.points[locator].position.to_array()})) {
                        Ok((_,b,m))=>json!({"mesh":mesh_index,"locator":locator,"source":style.source,"created":b.created,"living":b.living,"moving":m.moving,"spinning":m.spinning,"position_bounds":m.position_bounds,"maximum_speed":m.maximum_speed,"space":"world"}),
                        Err(error)=>json!({"mesh":mesh_index,"locator":locator,"error":error.to_string()}),
                    }));
                }
            }
        }
    }
    if !births.is_empty() {
        report["particle_births"] = json!(births);
        report["particle_motion"] = json!(motions);
    }
    if !world.is_empty() {
        report["particle_world"] = json!(world);
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use storm_render::model::{DynamicMesh, FireEmission, MAX_PARTICLE_INSTANCES};
    #[test]
    fn reports_check_the_total_particle_budget_and_omit_expired_geometry() {
        let mut asset = ModelAsset::ship_demo().unwrap();
        for mesh in &mut asset.meshes {
            if let Some(DynamicMesh::Particles(p)) = &mut mesh.dynamic {
                p.per_point = 100;
            }
        }
        asset.ship_state.native_fire = Some(
            (1..=MAX_PARTICLE_INSTANCES as u64)
                .map(|instance| FireEmission {
                    instance,
                    point: 0,
                    smoke: instance % 2 == 0,
                    started: 0.0,
                    stopped: Some(0.0),
                    profile: None,
                })
                .collect(),
        );
        asset.ship_state.validate(&asset.ship).unwrap();
        assert!(
            ship_frame(&asset, 0.0).is_err(),
            "individually valid mesh budgets must not bypass the scene total"
        );
        assert!(
            ship_frame(&asset, 8.0).is_ok(),
            "expired generations need no geometry or replay"
        );
    }
}
