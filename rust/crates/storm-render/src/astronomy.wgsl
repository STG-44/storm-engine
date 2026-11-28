struct View {
    project: mat4x4<f32>,
    eye_and_ratio: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var<uniform> parameters: vec4<f32>;
@group(1) @binding(1) var image: texture_2d<f32>;
@group(1) @binding(2) var image_sampler: sampler;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) offset_size: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) brightness: vec2<f32>,
) -> Vertex {
    let corners = array<vec2<f32>, 6>(
        vec2(-1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, -1.0),
        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0)
    );
    let corner = corners[vertex_index];
    let offset = offset_size.xyz * vec3(1.0, 1.0, -1.0);
    let center = view.eye_and_ratio.xyz + offset;
    var alpha = brightness.x;
    var clip: vec4<f32>;
    if parameters.y > 0.0 {
        // The original point scale uses viewport height and inverse distance.
        let point_size = offset_size.w / max(length(offset), 1e-6);
        clip = view.project * vec4(center, 1.0);
        let screen = corner * point_size * vec2(view.eye_and_ratio.w, 1.0) * clip.w;
        clip = vec4(clip.xy + screen, clip.zw);
        let factor = clamp((brightness.y + 2.0) / max(parameters.x + 2.0, 1e-6), 0.0, 1.0);
        alpha *= mix(1.0, 0.01, factor);
        alpha = round(alpha * 255.0) / 255.0;
    } else {
        let world = center + offset_size.w * (view.right.xyz * corner.x + view.up.xyz * corner.y);
        clip = view.project * vec4(world, 1.0);
    }
    clip.z = 0.999999 * clip.w;
    return Vertex(clip, corner * vec2(0.5, -0.5) + vec2(0.5), vec4(color.rgb, clamp(alpha, 0.0, 1.0)));
}

@fragment
fn fragment(input: Vertex) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, input.uv) * input.color;
}
