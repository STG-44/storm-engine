struct Camera {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye_time: vec4<f32>,
    light: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> model: mat4x4<f32>;
@group(1) @binding(0) var surface: texture_2d<f32>;
@group(1) @binding(1) var surface_sampler: sampler;

struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_position: vec3<f32>,
}

@vertex fn vertex_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
) -> Output {
    var output: Output;
    let world_position = model * vec4(position, 1.0);
    output.clip = camera.view_projection * world_position;
    output.normal = (model * vec4(normal, 0.0)).xyz;
    output.uv = uv;
    output.color = color;
    output.world_position = world_position.xyz;
    return output;
}

@fragment fn fragment_main(input: Output) -> @location(0) vec4<f32> {
    // Some original models (including cannon_x.gm) contain only zero normals.
    let face_normal = cross(dpdx(input.world_position), dpdy(input.world_position));
    let candidate = select(face_normal, input.normal, dot(input.normal, input.normal) > 1e-8);
    let normal = candidate * inverseSqrt(max(dot(candidate, candidate), 1e-12));
    let texel = textureSample(surface, surface_sampler, input.uv);
    if texel.a < 0.25 {
        discard;
    }
    let light = 0.35 + 0.65 * abs(dot(normal, normalize(camera.light.xyz)));
    return vec4(texel.rgb * input.color.rgb * light * vec3(1.02, 1.0, 0.96), 1.0);
}

@fragment fn particle_fragment(input: Output) -> @location(0) vec4<f32> {
    let texel=textureSample(surface,surface_sampler,input.uv);
    return vec4(texel.rgb*input.color.rgb,texel.a*input.color.a);
}
