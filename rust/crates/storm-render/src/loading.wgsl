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
    text_rect: vec4<f32>,
}
@group(0) @binding(4) var<uniform> loading: LoadingUniforms;
@group(0) @binding(5) var text_coverage: texture_2d<f32>;

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
    let inside = all(uv >= vec2(0.0)) && all(uv <= vec2(1.0));
    let front = edge + art * (1.0 - edge.a);
    let fade = loading.progress.w;
    if fade >= 0.0 {
        let faded_back = vec4(back.rgb, 1.0) * fade;
        if inside {
            let faded_front = front * fade;
            return faded_front + faded_back * (1.0 - faded_front.a);
        }
        return faded_back;
    }
    let text_uv = (in.uv - loading.text_rect.xy) / (loading.text_rect.zw - loading.text_rect.xy);
    let ink = textureSample(text_coverage, screen_sampler, text_uv).r;
    var screen = back;
    if inside {
        screen = front + back * (1.0 - front.a);
    }
    if loading.progress.z != 0.0 && all(text_uv >= vec2(0.0)) && all(text_uv <= vec2(1.0)) {
        let panel = premultiply(vec4(0.012, 0.028, 0.046, 0.92));
        let letters = premultiply(vec4(0.87, 0.95, 0.98, ink));
        let caption = letters + panel * (1.0 - letters.a);
        screen = caption + screen * (1.0 - caption.a);
    }
    if loading.progress.x >= 0.0 {
        let point = (in.uv - vec2(0.5, 0.935)) * vec2(loading.progress.y, 1.0);
        let radius = length(point);
        if radius > 0.01 && radius < 0.03 {
            let opacity = 1.0 - smoothstep(0.002, 0.0038, abs(radius - 0.02));
            let angle = atan2(point.y, point.x) / 6.2831853;
            let trail = pow(fract(angle - loading.progress.x), 4.0);
            let track = vec3(0.058, 0.11, 0.15);
            let highlight = vec3(0.12, 0.77, 0.69);
            let spinner = premultiply(vec4(mix(track, highlight, trail), opacity));
            screen = spinner + screen * (1.0 - spinner.a);
        }
    }
    return screen;
}

fn premultiply(color: vec4<f32>) -> vec4<f32> {
    return vec4(color.rgb * color.a, color.a);
}
