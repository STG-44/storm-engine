// Coefficients come from storm-sim::Waves; used by water and GPU agreement tests.
struct WaveField {
    components: array<vec4<f32>, 2>, // kx, kz, omega, amplitude
    phases: vec4<f32>,
}
@group(0) @binding(2) var<uniform> waves: WaveField;

// Height, dHeight/dX, dHeight/dZ.
fn wave_surface(position: vec2<f32>, time: f32) -> vec3<f32> {
    var result = vec3(0.0);
    for (var i = 0u; i < 2u; i += 1u) {
        let c = waves.components[i];
        let phase = dot(c.xy, position) - c.z * time + waves.phases[i];
        result += vec3(c.w * sin(phase), c.xy * (c.w * cos(phase)));
    }
    return result;
}
