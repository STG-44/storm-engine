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

fn sail_cutout(uv:vec2<f32>)->bool {
    if object.damage.x==0u {return false;}
    var cell=uv*vec2(3.0,4.0);
    var section=u32(clamp(floor(cell.x),0.0,2.0))*4u+u32(clamp(floor(cell.y),0.0,3.0));
    if object.damage.y!=0u {
        cell=vec2(uv.x*4.0,(uv.x+uv.y-1.0)*4.0);
        let row=u32(clamp(floor(cell.x),0.0,3.0));
        section=row*(row+1u)/2u+u32(clamp(floor(cell.y),0.0,f32(row)));
    }
    if (object.damage.x & (1u<<section))==0u {return false;}
    let p=fract(cell);
    let d=(p-vec2(0.5))/0.42;
    let edge=1.0+0.12*sin(p.x*43.0+p.y*31.0)*sin(p.y*37.0-p.x*29.0);
    return dot(d,d)<edge;
}
@fragment fn fragment_main(input: Output) -> @location(0) vec4<f32> {
    // Some original models (including cannon_x.gm) contain only zero normals.
    let face_normal = cross(dpdx(input.world_position), dpdy(input.world_position));
    let candidate = select(face_normal, input.normal, dot(input.normal, input.normal) > 1e-8);
    let normal = candidate * inverseSqrt(max(dot(candidate, candidate), 1e-12));
    let texel = textureSample(surface, surface_sampler, input.uv);
    if texel.a < 0.25 || sail_cutout(input.uv) {
        discard;
    }
    let light = 0.35 + 0.65 * abs(dot(normal, normalize(camera.light.xyz)));
    return vec4(texel.rgb * input.color.rgb * light * vec3(1.02, 1.0, 0.96), 1.0);
}

@fragment fn particle_fragment(input: Output) -> @location(0) vec4<f32> {
    let texel=textureSample(surface,surface_sampler,input.uv);
    return vec4(texel.rgb*input.color.rgb,texel.a*input.color.a);
}
