// Scalar SEA::WaveXZ over the same prepared maps used by ShipRocking.
// Renderer Z is reflected into native world coordinates at this boundary.
struct NativeSea {
    layers: array<vec4<f32>, 2>, // amplitude, scale, movement X/Z
    camera: vec4<f32>, // native camera X/Z, max distance, enabled
    bounds: vec4<f32>,
}
@group(0) @binding(3) var<storage, read> sea_maps: array<f32>;
@group(0) @binding(4) var<uniform> native_sea: NativeSea;

fn native_ffloor(v: f32) -> i32 {
    let x = v - 0.5;
    let lo = floor(x);
    let fraction = x - lo;
    var value = i32(lo);
    if fraction > 0.5 || (fraction == 0.5 && (value & 1) != 0) { value += 1; }
    return value;
}
fn sea_interpolate(base: u32, stride: u32, indices: vec4<u32>, f: vec2<f32>) -> f32 {
    let a = sea_maps[base + indices.x * stride];
    let b = sea_maps[base + indices.y * stride];
    let c = sea_maps[base + indices.z * stride];
    let d = sea_maps[base + indices.w * stride];
    return a + f.x * (b - a) + f.y * (c - a) + f.x * f.y * (d + a - b - c);
}
// Height and renderer-space normal. GPU precision is checked with tolerances.
fn native_sea_surface(position: vec2<f32>, with_normal: bool) -> vec4<f32> {
    let p = vec2(position.x, -position.y);
    let delta = p - native_sea.camera.xy;
    if dot(delta, delta) > native_sea.camera.z * native_sea.camera.z { return vec4(0.0, 0.0, 1.0, 0.0); }
    var height = 0.0;
    var normals: array<vec2<f32>, 2>;
    for (var l = 0u; l < 2u; l += 1u) {
        let layer = native_sea.layers[l];
        let point = (p + layer.zw) * layer.y;
        let ix = native_ffloor(point.x);
        let iz = native_ffloor(point.y);
        let f = point - vec2(f32(ix), f32(iz));
        let x0 = u32(ix & 127); let x1 = u32((ix + 1) & 127);
        let z0 = u32(iz & 127); let z1 = u32((iz + 1) & 127);
        let indices = vec4(x0 + z0 * 128u, x1 + z0 * 128u, x0 + z1 * 128u, x1 + z1 * 128u);
        let base = l * 49152u;
        height += layer.x * sea_interpolate(base, 1u, indices, f);
        if with_normal {
            normals[l] = vec2(sea_interpolate(base + 16384u, 2u, indices, f), sea_interpolate(base + 16385u, 2u, indices, f));
        }
    }
    var n = vec3(0.0, 1.0, 0.0);
    if with_normal {
        let a = normals[0]; let b = normals[1];
        let xz = native_sea.layers[0].y * native_sea.layers[0].x * a + native_sea.layers[1].y * native_sea.layers[1].x * b;
        let y = sqrt(1.0 - dot(a,a)) + sqrt(1.0 - dot(b,b));
        n = normalize(vec3(xz.x, y, -xz.y));
    }
    return vec4(height, n);
}
