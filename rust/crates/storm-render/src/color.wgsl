@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> ramp: array<vec4<f32>, 64>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, 1.0), vec2(-1.0, -3.0), vec2(3.0, 1.0));
    return vec4(positions[index], 0.0, 1.0);
}

fn correction(linear: f32) -> f32 {
    let srgb = select(1.055 * pow(linear, 1.0 / 2.4) - 0.055,
                      12.92 * linear, linear <= 0.0031308);
    let index = u32(round(clamp(srgb, 0.0, 1.0) * 255.0));
    let level = ramp[index / 4u][index % 4u];
    return select(pow((level + 0.055) / 1.055, 2.4), level / 12.92, level <= 0.04045);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let color = textureLoad(source, vec2<i32>(position.xy), 0);
    return vec4(correction(color.r), correction(color.g), correction(color.b), color.a);
}
