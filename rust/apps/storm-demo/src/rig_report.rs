use serde_json::{Value, json};
use storm_render::model::ModelAsset;
use storm_sim::{RigState, SailMode, SailTier};

pub fn report(asset: &ModelAsset) -> Value {
    let sails = asset
        .meshes
        .iter()
        .filter_map(|m| m.sail.as_ref())
        .map(|patch| {
            let origin = patch.origin.as_ref();
            json!({ "group": origin.map(|o| &o.group), "node": origin.map(|o| &o.node),
            "parent": origin.map(|o| &o.parent), "tier": patch.tier.name(),
            "triangular": patch.is_triangle(), "original_size_weight": patch.weight })
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
    json!({ "geometry_files": asset.geometry_files, "sail_count": sails.len(),
        "normal_count": normal, "full_only_count": sails.len()-normal,
        "normal_weight": layout.normal_weight, "full_only_weight": layout.full_only_weight,
        "normal_mode_fraction": RigState::for_mode(SailMode::Normal).fraction(layout), "sails": sails, "flags":flags,
        "rigging": { "ropes":r.ropes,"shrouds":r.shrouds,"flags":r.flags,
            "unmatched_ropes":r.unmatched_ropes,"incomplete_shrouds":r.incomplete_shrouds,
            "incomplete_flags":r.incomplete_flags,"invalid_lengths":r.invalid_lengths,"duplicate_points":r.duplicate_points } })
}
