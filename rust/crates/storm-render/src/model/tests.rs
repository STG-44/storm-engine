use super::*;

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn normal_mode_keeps_normal_sails_full_and_gathers_only_full_only_sails() {
    let mut asset = ModelAsset::sail_demo().unwrap();
    asset.textures[0].mips[0]
        .rgba
        .chunks_exact_mut(4)
        .for_each(|p| p.copy_from_slice(&[255, 0, 0, 255]));
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    camera.yaw = 0.0;
    let mut capture = |mode| {
        scene
            .set_rig_state(storm_sim::RigState::for_mode(mode))
            .unwrap();
        crate::render_offscreen_with(&gpu, 321, 181, 0.0, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                0.0,
                &camera,
                SailWind {
                    speed: 0.0,
                    direction: 0.0,
                },
            )
        })
        .unwrap()
    };
    let mask = |frame: &crate::Frame, left: bool| {
        frame
            .rgba
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, _)| (i % 321 < 160) == left)
            .map(|(_, p)| {
                if p[0] > 100 && p[1] < 20 && p[2] < 20 {
                    p[0]
                } else {
                    0
                }
            })
            .collect::<Vec<_>>()
    };
    let full = capture(storm_sim::SailMode::Full);
    let normal = capture(storm_sim::SailMode::Normal);
    let furled = capture(storm_sim::SailMode::Furled);
    assert_eq!(
        mask(&normal, true),
        mask(&full, true),
        "normal sail must retain its whole surface"
    );
    assert_eq!(
        mask(&normal, false),
        mask(&furled, false),
        "full-only sail must be completely gathered"
    );
    assert_ne!(mask(&normal, true), mask(&furled, true));
    assert_ne!(mask(&normal, false), mask(&full, false));
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn persistent_sail_buffers_furl_both_shapes_and_restore_the_full_rig() {
    let mut asset = ModelAsset::sail_demo().unwrap();
    asset.textures[0].mips[0]
        .rgba
        .chunks_exact_mut(4)
        .for_each(|p| p.copy_from_slice(&[255, 0, 0, 255]));
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
    let mut camera = OrbitCamera::new(&asset);
    camera.yaw = 0.0;
    let mut capture = |deployment, time, speed| {
        scene.set_rig_state(storm_sim::RigState::uniform(f64::from(deployment)))?;
        crate::render_offscreen_with(&gpu, 321, 181, time, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                time,
                &camera,
                SailWind {
                    speed,
                    direction: 0.0,
                },
            )
        })
    };
    let mask = |frame: &crate::Frame| {
        frame
            .rgba
            .chunks_exact(4)
            .map(|p| {
                if p[0] > 100 && p[1] < 20 && p[2] < 20 {
                    p[0]
                } else {
                    0
                }
            })
            .collect::<Vec<_>>()
    };
    let full = mask(&capture(1.0, 0.0, 0.0).unwrap());
    let half = mask(&capture(0.5, 0.0, 0.0).unwrap());
    let furled = mask(&capture(0.0, 0.0, 0.0).unwrap());
    for left in [true, false] {
        let count = |pixels: &[u8]| {
            pixels
                .iter()
                .enumerate()
                .filter(|&(i, &p)| p != 0 && (i % 321 < 160) == left)
                .count()
        };
        assert!(count(&full) > 300);
        assert!(count(&half) < count(&full) * 2 / 3);
        assert!(count(&half) > count(&full) / 3);
        assert!(count(&furled) > 0 && count(&furled) < count(&full) / 8);
    }
    assert_eq!(
        furled,
        mask(&capture(0.0, 7.0, 40.0).unwrap()),
        "furled cloth must stay still in changing wind/time"
    );
    assert_eq!(
        full,
        mask(&capture(1.0, 0.0, 0.0).unwrap()),
        "reusing buffers must restore both surfaces"
    );
    assert!(capture(f32::NAN, 0.0, 0.0).is_err());
}

#[test]
fn tilted_pose_keeps_bow_pitch_and_starboard_roll_signs_after_z_reflection() {
    for heading in [0.0, 0.7, 2.4] {
        let translation = Vec3::new(30.0, 1.0, -20.0);
        let pose = ModelPose {
            translation: translation.to_array(),
            yaw: heading + std::f32::consts::PI,
            pitch: 0.1,
            roll: -0.2,
        };
        let bow = pose.transform_point(-Vec3::Z).unwrap() - translation;
        let starboard = pose.transform_point(-Vec3::X).unwrap() - translation;
        assert!(bow.y > 0.09 && starboard.y > 0.19);
        assert!(bow.dot(Vec3::new(heading.sin(), 0.0, heading.cos())) > 0.98);
        let up = pose.transform_point(Vec3::Y).unwrap() - translation;
        assert!(bow.dot(up).abs() < 1e-5 && starboard.dot(up).abs() < 1e-5);
    }
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn displaced_water_depth_occludes_a_submerged_surface() {
    // A red horizontal plate alternates below a crest and above a trough.
    // Only geometry pixels count; animated shading alone cannot pass the test.
    let mut asset = ModelAsset::sail_demo().unwrap();
    asset.textures[0].mips[0]
        .rgba
        .chunks_exact_mut(4)
        .for_each(|p| p.copy_from_slice(&[255, 0, 0, 255]));
    asset.meshes = vec![Mesh {
        vertices: [
            [-1.0, 0.0, -1.0],
            [1.0, 0.0, -1.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
        ]
        .map(|position| ModelVertex {
            position,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            color: [1.0; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: 0,
        sail: None,
    }];
    asset.center = Vec3::ZERO;
    asset.radius = 4.0;
    let camera = OrbitCamera::new(&asset);
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let waves = storm_sim::Waves {
        height_m: 4.0,
        direction: 0.0,
    };
    let capture = |time| {
        crate::render_sailing_offscreen(
            &gpu,
            &asset,
            &camera,
            SailWind::default(),
            ModelPose::default(),
            waves,
            storm_sim::RigState::uniform(1.0),
            257,
            145,
            time,
        )
        .unwrap()
    };
    let red = |f: &crate::Frame| {
        f.rgba
            .chunks_exact(4)
            .filter(|p| p[0] > 100 && p[1] < 20 && p[2] < 20)
            .count()
    };
    assert_eq!(
        red(&capture(0.0)),
        0,
        "plate must be hidden under the crest"
    );
    assert!(
        red(&capture(3.0)) > 100,
        "plate must emerge above the trough"
    );
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn water_shader_agrees_with_cpu_height_and_gradient() {
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wave agreement probe"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../waves.wgsl"),
                    r#"
@group(0) @binding(3) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = f32(id.x);
    output[id.x] = vec4(wave_surface(vec2(i * 0.71, i * -0.37), 360.0 + i * 0.29), 0.0);
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
    for height_m in [0.0, 4.0] {
        let waves = storm_sim::Waves {
            height_m,
            direction: 0.83,
        };
        let uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&WaveUniform::from(waves)),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64 * 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
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
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 64 * 16);
        gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        for (i, result) in bytemuck::cast_slice::<u8, [f32; 4]>(&mapped)
            .iter()
            .enumerate()
        {
            let i = i as f64;
            let (h, g) = waves.sample(glam::DVec2::new(i * 0.71, i * -0.37), 360.0 + i * 0.29);
            for (gpu, cpu) in result[..3].iter().zip([h, g.x, g.y]) {
                assert!(
                    (f64::from(*gpu) - cpu).abs() < 0.0002,
                    "GPU {gpu}, CPU {cpu}"
                );
            }
        }
        drop(mapped);
        readback.unmap();
    }
}
