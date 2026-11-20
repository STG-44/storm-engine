struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var screen_sampler: sampler;
@group(0) @binding(1) var background: texture_2d<f32>;
@group(0) @binding(2) var picture: texture_2d<f32>;
@group(0) @binding(3) var border: texture_2d<f32>;
struct LoadingUniforms {
    picture_rect: vec4<f32>,
    progress: vec4<f32>,
}
@group(0) @binding(4) var<uniform> loading: LoadingUniforms;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOut {
    let positions = array<vec2<f32>, 3>(
        vec2(-1.0, 1.0), vec2(-1.0, -3.0), vec2(3.0, 1.0)
    );
    let position = positions[index];
    return VertexOut(vec4(position, 0.0, 1.0), position * vec2(0.5, -0.5) + vec2(0.5));
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let back = premultiply(textureSample(background, screen_sampler, in.uv));
    let rect = loading.picture_rect;
    let uv = (in.uv - rect.xy) / (rect.zw - rect.xy);
    let art = premultiply(textureSample(picture, screen_sampler, uv));
    let edge = premultiply(textureSample(border, screen_sampler, uv));
    var screen = back;
    if all(uv >= vec2(0.0)) && all(uv <= vec2(1.0)) {
        let front = edge + art * (1.0 - edge.a);
        screen = front + back * (1.0 - front.a);
    }
    if loading.progress.x >= 0.0 {
        let track = (in.uv - vec2(0.25, 0.916)) / vec2(0.5, 0.014);
        if all(track >= vec2(0.0)) && all(track <= vec2(1.0)) {
            let center = mix(0.13, 0.87, 1.0 - abs(2.0 * loading.progress.x - 1.0));
            let segment = 1.0 - smoothstep(0.095, 0.13, abs(track.x - center));
            let dark = vec3(0.035, 0.074, 0.112);
            let accent = vec3(0.10, 0.68, 0.63);
            return vec4(mix(dark, accent, segment), 1.0);
        }
    }
    return screen;
}

fn premultiply(color: vec4<f32>) -> vec4<f32> {
    return vec4(color.rgb * color.a, color.a);
}
