use super::*;
use crate::Gpu;
use std::sync::{Arc, mpsc};
use wgpu::util::DeviceExt;

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn prepared_sea_gpu_height_normal_and_reflected_coordinates_agree_with_cpu() {
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native SEA query agreement"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../../native_sea.wgsl"),
                    r#"
@group(0) @binding(5) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    var p = vec2((f32(id.x) - 32.0) * 7.125, (f32(id.x) - 29.0) * 2.375);
    if id.x == 62u { p = vec2(1607.0, 9.0); }
    if id.x == 63u { p = vec2(1607.125, 9.0); }
    output[id.x] = native_sea_surface(p,true);
}
"#
                )
                .into(),
            ),
        });
    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let frames = Arc::new(storm_sim::sea::Frames::fixture());
    for amplitude in [[8.0, 1.0], [0.0, 0.0], [-2.0, 0.75]] {
        let mut sea = storm_sim::sea::Sea::new(frames.clone(), Default::default()).unwrap();
        sea.enable_weather();
        let mut attributes = storm_core::AttributeStore::default();
        let root = attributes.create_root().unwrap();
        for (path, value) in [
            ("Sea2.Amp1", amplitude[0].to_string()),
            ("Sea2.Amp2", amplitude[1].to_string()),
            ("MaxSeaHeight", "50".into()),
            ("MaxSeaDistance", "-1600".into()),
        ] {
            let changed = attributes.create_path(root, path).unwrap();
            attributes.set_value(changed, Some(&value)).unwrap();
            sea.attribute_changed(&attributes, root, changed).unwrap();
        }
        sea.set_camera([7.0, -9.0]).unwrap();
        for _ in 0..11 {
            sea.advance(1000).unwrap();
        }
        sea.advance(17).unwrap();
        let s = sea.surface();
        let maps = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(s.maps()),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&Uniform::from(s)),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: maps.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 1024);
        gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::sync_channel(1);
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let mapped = read.slice(..).get_mapped_range().unwrap();
        for (i, v) in bytemuck::cast_slice::<u8, [f32; 4]>(&mapped)
            .iter()
            .enumerate()
        {
            let [x, z] = match i {
                62 => [1607.0, 9.0],
                63 => [1607.125, 9.0],
                _ => [(i as f32 - 32.0) * 7.125, (i as f32 - 29.0) * 2.375],
            };
            let sample = s.sample(x, -z).unwrap();
            let n = sample.normal;
            for (actual, expected) in v.iter().zip([sample.height, n[0], n[1], -n[2]]) {
                assert!(
                    (*actual - expected).abs() < 0.0001,
                    "query {i}: GPU {actual}, CPU {expected}"
                );
            }
        }
        drop(mapped);
        read.unmap();
    }
}
