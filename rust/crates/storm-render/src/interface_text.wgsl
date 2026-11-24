@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rgba: vec4<f32>,
};

@vertex
fn vertex(
    @builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) rgba: vec4<f32>,
) -> Output {
    let corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0),
    )[index];
    return Output(vec4<f32>(mix(rect.xy, rect.zw, corner), 0.0, 1.0),
                  mix(uv.xy, uv.zw, corner), rgba);
}

@fragment
fn fragment(input: Output) -> @location(0) vec4<f32> {
    return textureSample(atlas, atlas_sampler, input.uv) * input.rgba;
}
