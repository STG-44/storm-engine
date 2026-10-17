// Resource-free renderer diagnostic. This is not the original Storm sea model.
@group(0) @binding(0) var<uniform> frame: vec4<f32>;

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let vertices = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(vertices[index], 0.0, 1.0);
}

fn sky(ray: vec3<f32>) -> vec3<f32> {
    let sun = normalize(vec3(0.42, 0.16, -1.0));
    let height = clamp(ray.y * 1.7, 0.0, 1.0);
    let atmosphere = mix(vec3(0.68, 0.78, 0.78), vec3(0.08, 0.27, 0.45), sqrt(height));
    let sun_dot = max(dot(ray, sun), 0.0);
    let glow = vec3(1.0, 0.67, 0.32) * pow(sun_dot, 24.0) * 0.35;
    let disk = vec3(1.0, 0.85, 0.58) * smoothstep(0.9995, 0.9999, sun_dot) * 2.0;
    return atmosphere + glow + disk;
}

@fragment
fn fragment_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (pixel.xy / frame.xy) * 2.0 - 1.0;
    let ray = normalize(vec3(uv.x * frame.x / frame.y, -uv.y * 0.7 + 0.12, -1.6));
    if ray.y >= -0.002 { return vec4(sky(ray), 1.0); }
    let distance = 2.8 / -ray.y;
    let point = ray.xz * distance;
    var gradient = vec2(0.0);
    var amplitude = 0.13;
    var frequency = 0.45;
    var direction = normalize(vec2(0.7, 0.5));
    for (var octave = 0u; octave < 6u; octave += 1u) {
        let phase = dot(point, direction) * frequency + frame.z * (0.6 + f32(octave) * 0.22);
        gradient += direction * cos(phase + sin(phase * 0.63)) * amplitude;
        direction = vec2(direction.x * 0.6 - direction.y * 0.8, direction.x * 0.8 + direction.y * 0.6);
        amplitude *= 0.62;
        frequency *= 1.93;
    }
    let normal = normalize(vec3(-gradient.x, 1.0, -gradient.y));
    let reflected = reflect(ray, normal);
    let fresnel = 0.035 + 0.965 * pow(1.0 - max(dot(-ray, normal), 0.0), 5.0);
    let body = mix(vec3(0.012, 0.085, 0.11), vec3(0.025, 0.22, 0.25),
        clamp(normal.y * 0.6 + gradient.x * 1.3, 0.0, 1.0));
    let sun = normalize(vec3(0.42, 0.16, -1.0));
    let specular = pow(max(dot(reflected, sun), 0.0), 160.0);
    var water = mix(body, sky(reflected), fresnel) + vec3(1.0, 0.73, 0.38) * specular * 1.5;
    water = mix(water, vec3(0.51, 0.65, 0.67), 1.0 - exp(-distance * 0.0018));
    return vec4(water, 1.0);
}
