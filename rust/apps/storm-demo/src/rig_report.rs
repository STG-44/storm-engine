use serde_json::{Value, json};
use storm_render::model::ModelAsset;
use storm_sim::{RigState, SailMode, SailTier};

pub fn report(asset: &ModelAsset) -> Value {
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
    json!({ "sail_damage_texture":asset.sail_damage.as_ref().map(|m|&m.source),"geometry_files": asset.geometry_files, "sail_count": sails.len(),
        "normal_count": normal, "full_only_count": sails.len()-normal,
        "normal_weight": layout.normal_weight, "full_only_weight": layout.full_only_weight,"missing_sail_weight":layout.missing_weight,
        "normal_mode_fraction": RigState::for_mode(SailMode::Normal).fraction(layout), "sails": sails, "flags":flags,
        "ship":ship_frame(asset,0.0),
        "rigging": { "ropes":r.ropes,"shrouds":r.shrouds,"flags":r.flags,
            "unmatched_ropes":r.unmatched_ropes,"incomplete_shrouds":r.incomplete_shrouds,
            "incomplete_flags":r.incomplete_flags,"invalid_lengths":r.invalid_lengths,"duplicate_points":r.duplicate_points } })
}

pub fn ship_frame(asset: &ModelAsset, seconds: f32) -> Value {
    let state = &asset.ship_state;
    let mut report = json!({"crew_count":asset.ship.crew_count,"crew_visible":state.crew_visible,"waypoint_counts":asset.ship.waypoint_counts,"waypoint_missing_links":asset.ship.waypoint_missing_links,"crew_source":asset.ship.crew_source,
        "masts":asset.ship.masts(),"hulls":asset.ship.hulls(),
        "fire_points":asset.ship.fires.iter().map(|p|json!({"node":p.node,"name":p.name,"position":p.position.to_array()})).collect::<Vec<_>>(),
        "sail_holes":state.sail_holes,"broken_masts":state.broken_masts,"broken_hulls":state.broken_hulls,"fire_started":state.fire_started,"seconds":seconds,
        "crew_positions":asset.meshes.iter().filter_map(|m|match &m.dynamic {
            Some(storm_render::model::DynamicMesh::Crew(c))=>{let(p,_)=c.route.sample(seconds+c.phase);Some([p.x,p.y,-p.z])},_=>None
        }).collect::<Vec<_>>()});
    let profiles=asset.meshes.iter().filter_map(|m| match &m.dynamic {Some(storm_render::model::DynamicMesh::Particles(p))=>p.style.as_ref().map(|s|json!({"source":s.source,"kind":if p.smoke {"smoke"}else{"fire"},"slots_per_locator":p.per_point,"maximum_lifetime":p.lifetime,"uv_frames":s.frames.len()})),_=>None}).collect::<Vec<_>>();
    if !profiles.is_empty() {
        report["xps_particle_styles"] = json!(profiles);
    }
    report
}
