@group(0) @binding(0) var<uniform> view_projection: mat4x4<f32>;
@group(1) @binding(0) var foam_image: texture_2d<f32>;
@group(1) @binding(1) var foam_sampler: sampler;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) alpha: f32) -> Output {
    return Output(view_projection * vec4<f32>(position, 1.0), uv, alpha);
}

@fragment
fn fragment(input: Output) -> @location(0) vec4<f32> {
    let foam = textureSample(foam_image, foam_sampler, input.uv);
    // The original CoastFoam technique selects the texture RGB and modulates alpha twice.
    return vec4<f32>(foam.rgb, saturate(2.0 * foam.a * input.alpha));
}
