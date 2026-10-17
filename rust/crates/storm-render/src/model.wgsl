struct Camera {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye_time: vec4<f32>,
    light: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
struct Object { matrix:mat4x4<f32>, damage:vec4<u32>, }
@group(0) @binding(1) var<uniform> object: Object;
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
    let world_position = object.matrix * vec4(position, 1.0);
    output.clip = camera.view_projection * world_position;
    output.normal = (object.matrix * vec4(normal, 0.0)).xyz;
    output.uv = uv;
    output.color = color;
    output.world_position = world_position.xyz;
    return output;
}

@group(1) @binding(2) var sail_mask:texture_2d<f32>;
@group(1) @binding(3) var mask_wrap:sampler;
@group(1) @binding(4) var mask_mirror_u:sampler;
fn sail_mask_uv(uv:vec2<f32>)->vec2<f32> {
    // SAILONE::SetTexGrid tu3/tv3. V offset distinguishes the two topologies.
    return vec2(1.0+uv.x*0.5,uv.y*0.5+select(0.0,0.5,object.damage.y!=0u));
}
fn shade(input:Output,texel:vec4<f32>,alpha:f32)->vec4<f32> {
    let face_normal=cross(dpdx(input.world_position),dpdy(input.world_position));
    let candidate=select(face_normal,input.normal,dot(input.normal,input.normal)>1e-8);
    let normal=candidate*inverseSqrt(max(dot(candidate,candidate),1e-12));
    let light=0.35+0.65*abs(dot(normal,normalize(camera.light.xyz)));
    return vec4(texel.rgb*input.color.rgb*light*vec3(1.02,1.0,0.96),alpha);
}
@fragment fn sail_intact_fragment(input:Output)->@location(0) vec4<f32> {
    let texel=textureSample(surface,surface_sampler,input.uv);
    let alpha=texel.a*textureSample(sail_mask,mask_wrap,sail_mask_uv(input.uv)).a;
    if alpha<=160.0/255.0 {discard;}
    return shade(input,texel,alpha);
}
@fragment fn sail_hole_fragment(input:Output)->@location(0) vec4<f32> {
    let texel=textureSample(surface,surface_sampler,input.uv);
    let alpha=texel.a*textureSample(sail_mask,mask_mirror_u,sail_mask_uv(input.uv)).a;
    if alpha<=160.0/255.0 {discard;}
    return shade(input,texel,alpha);
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
