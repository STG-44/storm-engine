@group(0) @binding(0) var sprite_texture: texture_2d<f32>;
@group(0) @binding(1) var sprite_sampler: sampler;

struct SpriteVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) height: f32,
}

@vertex
fn vertex(
    @location(0) clip: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) height: f32,
) -> SpriteVertex {
    return SpriteVertex(clip, uv, color.rgb, height);
}

@fragment
fn fragment(sprite: SpriteVertex) -> @location(0) vec4<f32> {
    let texel = textureSample(sprite_texture, sprite_sampler, sprite.uv);
    if sprite.height < 0.0 {
        discard;
    }
    return vec4(texel.rgb * sprite.color, 1.0);
}
