@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rgba: vec4<f32>,
    @location(2) @interpolate(flat) color_mode: u32,
};

@vertex
fn vertex(
    @builtin(vertex_index) index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) rgba: vec4<f32>,
    @location(3) color_mode: u32,
) -> Output {
    let corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0),
    )[index];
    return Output(vec4<f32>(mix(rect.xy, rect.zw, corner), 0.0, 1.0),
                  mix(uv.xy, uv.zw, corner), rgba, color_mode);
}

@fragment
fn fragment(input: Output) -> @location(0) vec4<f32> {
    let texel = textureSample(atlas, atlas_sampler, input.uv);
    if input.color_mode != 0u {
        var tinted = input.rgba.rgb;
        if input.color_mode == 1u {
            // iVideo, iTitle and iButton modulate encoded RGB by twice the diffuse color.
            let srgb = select(12.92 * texel.rgb, 1.055 * pow(texel.rgb, vec3<f32>(1.0 / 2.4)) - 0.055,
                              texel.rgb > vec3<f32>(0.0031308));
            tinted = clamp(srgb * input.rgba.rgb * 2.0, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        let linear = select(tinted / 12.92, pow((tinted + 0.055) / 1.055, vec3<f32>(2.4)),
                            tinted > vec3<f32>(0.04045));
        return vec4<f32>(linear, texel.a * input.rgba.a);
    }
    return texel * input.rgba;
}
