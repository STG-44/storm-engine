@group(0) @binding(0) var<uniform> view_projection: mat4x4<f32>;

struct Streak {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vertex(
    @location(0) position_and_opacity: vec4<f32>,
    @location(1) world_x: vec4<f32>,
    @location(2) world_y: vec4<f32>,
    @location(3) world_z: vec4<f32>,
    @location(4) world_translation: vec4<f32>,
    @location(5) color: vec4<f32>,
) -> Streak {
    let world = mat4x4<f32>(world_x, world_y, world_z, world_translation);
    let position = view_projection * (world * vec4(position_and_opacity.xyz, 1.0));
    return Streak(position, vec4(color.rgb, color.a * position_and_opacity.w));
}

@fragment
fn fragment(streak: Streak) -> @location(0) vec4<f32> {
    return streak.color;
}
