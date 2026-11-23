struct View {
    sky: mat4x4<f32>,
    fog: mat4x4<f32>,
    blend: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var current_sky: texture_2d<f32>;
@group(1) @binding(1) var next_sky: texture_2d<f32>;
@group(1) @binding(2) var sky_sampler: sampler;

struct Face {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv_blend: vec3<f32>,
}

struct Fog {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn face_vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Face {
    let clip = view.sky * vec4(position, 1.0);
    return Face(vec4(clip.xy, clip.w, clip.w), vec3(uv, view.blend.x));
}

@fragment
fn face_fragment(face: Face) -> @location(0) vec4<f32> {
    let current = textureSample(current_sky, sky_sampler, face.uv_blend.xy);
    let next = textureSample(next_sky, sky_sampler, face.uv_blend.xy);
    return mix(current, next, face.uv_blend.z);
}

@vertex
fn fog_vertex(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> Fog {
    let clip = view.fog * vec4(position, 1.0);
    return Fog(vec4(clip.xy, clip.w, clip.w), color);
}

@fragment
fn fog_fragment(fog: Fog) -> @location(0) vec4<f32> {
    return fog.color;
}
