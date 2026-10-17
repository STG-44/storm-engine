// Preview water only: the original Storm sea simulation has not been ported.
struct Camera {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye_time: vec4<f32>,
    light: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex fn vertex_main(@builtin(vertex_index) index: u32) -> Vertex {
    let p = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0))[index];
    var output: Vertex;
    output.position = vec4(p, 0.0, 1.0);
    output.uv = vec2(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return output;
}

fn sky(ray: vec3<f32>) -> vec3<f32> {
    let sun = pow(max(dot(ray, normalize(camera.light.xyz)), 0.0), 700.0);
    return mix(vec3(0.62, 0.78, 0.84), vec3(0.13, 0.36, 0.59), sqrt(max(ray.y, 0.0)))
        + sun * vec3(2.0, 1.7, 1.2);
}

struct Pixel {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@fragment fn fragment_main(input: Vertex) -> Pixel {
    let far = camera.inverse_view_projection
        * vec4(input.uv.x * 2.0 - 1.0, 1.0 - input.uv.y * 2.0, 1.0, 1.0);
    let eye = camera.eye_time.xyz;
    let ray = normalize(far.xyz / far.w - eye);
    var output: Pixel;
    output.color = vec4(sky(ray), 1.0);
    output.depth = 1.0;
    if ray.y < -0.0001 && eye.y > 0.0 {
        let distance = -eye.y / ray.y;
        let p = eye + ray * distance;
        let time = camera.eye_time.w;
        let fade = 1.0 - smoothstep(camera.light.w * 3.0, camera.light.w * 12.0, distance);
        let normal = normalize(vec3(
            fade * (0.055 * cos(p.x * 0.6 + p.z * 0.28 - time * 1.1)
                + 0.025 * sin(p.z * 1.9 + time)),
            1.0,
            fade * 0.06 * cos(p.z * 0.7 + p.x * 0.18 - time * 0.8),
        ));
        let fresnel = 0.025 + 0.975 * pow(1.0 - max(dot(-ray, normal), 0.0), 5.0);
        let reflection = sky(reflect(ray, normal));
        let specular = pow(max(dot(reflect(-normalize(camera.light.xyz), normal), -ray), 0.0), 120.0);
        let water = mix(vec3(0.012, 0.095, 0.125), reflection, fresnel)
            + specular * vec3(0.9, 0.8, 0.56);
        let haze = 1.0 - exp(-distance / (camera.light.w * 120.0));
        output.color = vec4(mix(water, vec3(0.62, 0.78, 0.84), haze), 1.0);
        let clip = camera.view_projection * vec4(p, 1.0);
        output.depth = clamp(clip.z / clip.w, 0.0, 1.0);
    }
    return output;
}
