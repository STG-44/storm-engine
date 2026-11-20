struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var screen_sampler: sampler;
@group(0) @binding(1) var background: texture_2d<f32>;
@group(0) @binding(2) var picture: texture_2d<f32>;
@group(0) @binding(3) var border: texture_2d<f32>;
@group(0) @binding(4) var<uniform> picture_rect: vec4<f32>;

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
    let uv = (in.uv - picture_rect.xy) / (picture_rect.zw - picture_rect.xy);
    let art = premultiply(textureSample(picture, screen_sampler, uv));
    let edge = premultiply(textureSample(border, screen_sampler, uv));
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
        return back;
    }

    let front = edge + art * (1.0 - edge.a);
    return front + back * (1.0 - front.a);
}

fn premultiply(color: vec4<f32>) -> vec4<f32> {
    return vec4(color.rgb * color.a, color.a);
}
