//! Explicit static-grid diagnostic; all three local UV sets remain inspectable.
use super::*;
pub(super) fn report(assets: &[ModelAsset]) -> Result<Json> {
    let bodies=assets.iter().map(|asset|{
        let sails=asset.meshes.iter().enumerate().filter_map(|(i,m)|m.sail.as_ref().map(|s|(i,s))).map(|(i,s)|{
            let (state,sphere)=s.native_geometry(asset.ship_state.sail_holes.get(&i).copied().unwrap_or(0))?;
            let vertices=storm_sim::sail_geometry::fill(&state,sphere,[1.0;2])?;
            Ok(json!({"mesh":i,"kind":state.kind as i32,"geometry_bits":state.geometry.map(|v|v.map(f32::to_bits)),"sphere_bits":sphere.center.into_iter().chain([sphere.radius]).map(f32::to_bits).collect::<Vec<_>>(),"vertex_bits":vertices.iter().map(|v|v.position.into_iter().chain(v.normal).chain(v.uv.into_iter().flatten()).map(f32::to_bits).collect::<Vec<_>>()).collect::<Vec<_>>()}))
        }).collect::<Result<Vec<_>>>()?;
        Ok(json!({"sails":sails}))
    }).collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"mode":"static_full_grid","depth_provider":"no_limit_point_fixed_half_samples","native_waves":false,"native_rolling_vertices":false,"bodies":bodies}),
    )
}
