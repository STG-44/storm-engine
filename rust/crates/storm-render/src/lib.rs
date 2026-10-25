//! Native graphics foundation for the Storm Engine Rust port.

use anyhow::{Context, Result, ensure};
use std::{fs::File, io::BufWriter, path::Path, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

pub const DIAGNOSTIC_SHADER: &str = include_str!("sea.wgsl");
pub mod model;

/// Reject unavailable backends instead of silently selecting a different API.
pub fn create_instance(backends: wgpu::Backends) -> Result<wgpu::Instance> {
    let enabled = backends & wgpu::Instance::enabled_backend_features();
    ensure!(
        !enabled.is_empty(),
        "requested graphics API is unavailable on this platform"
    );
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = enabled;
    Ok(wgpu::Instance::new(descriptor))
}

pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn request(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .context("no compatible graphics adapter; check the selected API and GPU driver")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Storm Rust device"),
                ..Default::default()
            })
            .await
            .context("creating the graphics device")?;
        Ok(Self {
            adapter,
            device,
            queue,
        })
    }
}

/// A resource-free scene to exercise the pipeline before original assets are available.
/// This is separate from the future Storm sea simulation.
pub struct DiagnosticScene {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl DiagnosticScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Diagnostic sea WGSL"),
            source: wgpu::ShaderSource::Wgsl(DIAGNOSTIC_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Diagnostic sea pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frame dimensions and time"),
            contents: bytemuck::cast_slice(&[1.0_f32, 1.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Diagnostic frame"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            uniforms,
            bind_group,
        }
    }

    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        seconds: f32,
    ) {
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, seconds, 0.0]),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Diagnostic sea pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn save_png(&self, path: &Path) -> Result<()> {
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("writing PNG header")?;
        writer
            .write_image_data(&self.rgba)
            .context("writing PNG pixels")?;
        writer.finish().context("finishing PNG")
    }
}

/// Render through the window's scene pipeline, then read back RGBA8 without row padding.
pub fn render_offscreen(gpu: &Gpu, width: u32, height: u32, seconds: f32) -> Result<Frame> {
    let scene = DiagnosticScene::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    render_offscreen_with(gpu, width, height, seconds, |encoder, target| {
        scene.draw(&gpu.queue, encoder, target, width, height, seconds);
        Ok(())
    })
}

pub fn render_model_offscreen(
    gpu: &Gpu,
    asset: &model::ModelAsset,
    camera: &model::OrbitCamera,
    wind: model::SailWind,
    width: u32,
    height: u32,
    seconds: f32,
) -> Result<Frame> {
    render_model_offscreen_at_pose(
        gpu,
        asset,
        camera,
        wind,
        model::ModelPose::default(),
        width,
        height,
        seconds,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_model_offscreen_at_pose(
    gpu: &Gpu,
    asset: &model::ModelAsset,
    camera: &model::OrbitCamera,
    wind: model::SailWind,
    pose: model::ModelPose,
    width: u32,
    height: u32,
    seconds: f32,
) -> Result<Frame> {
    render_sailing_offscreen(
        gpu,
        asset,
        camera,
        wind,
        pose,
        storm_sim::Waves::default(),
        storm_sim::RigState::uniform(1.0),
        width,
        height,
        seconds,
    )
}

/// Offscreen model with the same analytic sea as the CPU hull response.
#[allow(clippy::too_many_arguments)]
pub fn render_sailing_offscreen(
    gpu: &Gpu,
    asset: &model::ModelAsset,
    camera: &model::OrbitCamera,
    wind: model::SailWind,
    pose: model::ModelPose,
    waves: storm_sim::Waves,
    rig: storm_sim::RigState,
    width: u32,
    height: u32,
    seconds: f32,
) -> Result<Frame> {
    let mut scene = model::ModelScene::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb, asset)?;
    scene.set_pose(pose)?;
    scene.set_waves(waves)?;
    scene.set_rig_state(rig)?;
    render_offscreen_with(gpu, width, height, seconds, |encoder, target| {
        scene.draw(gpu, encoder, target, width, height, seconds, camera, wind)
    })
}

/// Render independent ships through shared depth and one ocean pass.
pub fn render_fleet_offscreen(
    gpu: &Gpu,
    scenes: &mut [model::ModelScene],
    camera: &model::OrbitCamera,
    wind: model::SailWind,
    width: u32,
    height: u32,
    seconds: f32,
) -> Result<Frame> {
    render_offscreen_with(gpu, width, height, seconds, |encoder, target| {
        model::ModelScene::draw_fleet(
            scenes, gpu, encoder, target, width, height, seconds, camera, wind,
        )
    })
}

fn render_offscreen_with(
    gpu: &Gpu,
    width: u32,
    height: u32,
    seconds: f32,
    draw: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView) -> Result<()>,
) -> Result<Frame> {
    let limits = gpu.device.limits();
    ensure!(width > 0 && height > 0, "frame dimensions must be nonzero");
    ensure!(
        width <= limits.max_texture_dimension_2d && height <= limits.max_texture_dimension_2d,
        "frame dimensions exceed this GPU's texture limit"
    );
    ensure!(seconds.is_finite(), "frame time must be finite");
    let row_bytes = width.checked_mul(4).context("frame row is too large")?;
    let padded_row = row_bytes
        .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        .checked_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        .context("frame row is too large")?;
    let buffer_size = u64::from(padded_row) * u64::from(height);
    ensure!(
        buffer_size <= limits.max_buffer_size,
        "frame exceeds this GPU's readback buffer limit"
    );
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Offscreen diagnostic frame"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Frame readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    draw(&mut encoder, &texture.create_view(&Default::default()))?;
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .context("waiting for GPU frame readback")?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .context("GPU map callback did not complete")?
        .context("mapping frame readback")?;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .context("reading mapped frame")?;
    let mut rgba = Vec::with_capacity((u64::from(row_bytes) * u64::from(height)) as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        rgba.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    readback.unmap();
    ensure!(
        rgba.len() == row_bytes as usize * height as usize,
        "GPU readback has an unexpected length"
    );
    Ok(Frame {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_shader_validates_without_a_gpu() {
        for shader in [DIAGNOSTIC_SHADER, model::MODEL_SHADER, model::OCEAN_SHADER] {
            let module = naga::front::wgsl::parse_str(shader).expect("valid WGSL");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .expect("portable shader capabilities");
        }
    }

    #[test]
    #[ignore = "requires a native GPU or a software Vulkan adapter"]
    fn offscreen_frame_has_pixels_animation_and_correct_row_alignment() {
        let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
        let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
        // 65 RGBA pixels require GPU row padding; CPU output must omit it.
        let first = render_offscreen(&gpu, 65, 33, 0.0).unwrap();
        let later = render_offscreen(&gpu, 65, 33, 8.0).unwrap();
        assert_eq!(first.rgba.len(), 65 * 33 * 4);
        assert!(first.rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert!(first.rgba.chunks_exact(4).any(|p| p != &first.rgba[..4]));
        assert_ne!(
            first.rgba, later.rgba,
            "wave animation must affect the image"
        );
        assert!(render_offscreen(&gpu, 0, 33, 0.0).is_err());
        assert!(render_offscreen(&gpu, 65, 33, f32::NAN).is_err());
        assert!(render_offscreen(&gpu, u32::MAX, 33, 0.0).is_err());
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn original_mesh_rasterizes_textures_depth_and_orbit() {
        use model::{LoadOptions, ModelAsset, OrbitCamera};
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/modding-tool/Meshes/cannon_x.gm");
        let mut asset = ModelAsset::load(
            &path,
            &LoadOptions {
                untextured: true,
                ..Default::default()
            },
        )
        .unwrap();
        asset.textures[0].mips[0].rgba = vec![255, 0, 0, 255];
        let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
        let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
        let mut camera = OrbitCamera::new(&asset);
        let frame = render_model_offscreen(
            &gpu,
            &asset,
            &camera,
            model::SailWind::default(),
            167,
            93,
            0.0,
        )
        .unwrap();
        let red = frame
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] > 100 && p[1] < 20 && p[2] < 20)
            .count();
        if red <= 50 {
            let path = std::env::temp_dir().join("storm-model-failed.png");
            frame.save_png(&path).unwrap();
            eprintln!("Failed frame: {}", path.display());
        }
        assert!(
            red > 50,
            "mesh/texture/depth pipeline did not produce red geometry: {red}"
        );
        assert_eq!(frame.rgba.len(), 167 * 93 * 4);
        let background = ModelAsset {
            sail_damage: None,
            ship: Default::default(),
            ship_state: Default::default(),
            meshes: vec![],
            textures: vec![],
            center: asset.center,
            radius: asset.radius,
            geometry_files: 0,
            sails: 0,
            rigging_stats: model::RiggingStats::default(),
        };
        let sky = render_model_offscreen(
            &gpu,
            &background,
            &camera,
            model::SailWind::default(),
            167,
            93,
            0.0,
        )
        .unwrap();
        assert_ne!(frame.rgba, sky.rgba, "model must alter the background");
        camera.orbit(90.0, 12.0);
        camera.zoom(1.0);
        let moved = render_model_offscreen(
            &gpu,
            &asset,
            &camera,
            model::SailWind::default(),
            167,
            93,
            0.0,
        )
        .unwrap();
        assert_ne!(frame.rgba, moved.rgba);
        camera.distance = f32::NAN;
        assert!(
            render_model_offscreen(
                &gpu,
                &asset,
                &camera,
                model::SailWind::default(),
                167,
                93,
                0.0
            )
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn both_sail_topologies_render_and_wind_changes_the_cloth_on_gpu() {
        use model::{ModelAsset, OrbitCamera, SailWind};
        let mut asset = ModelAsset::sail_demo().unwrap();
        for pixel in asset.textures[0].mips[0].rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[255, 0, 0, 255]);
        }
        let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
        let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
        let mut camera = OrbitCamera::new(&asset);
        camera.yaw = 0.0; // Separate left quad and right triangle on screen.
        let wind = SailWind {
            speed: 24.0,
            direction: 0.0,
        };
        let mut scene =
            model::ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
        let mut capture = |wind, seconds| {
            render_offscreen_with(&gpu, 257, 145, seconds, |encoder, target| {
                scene.draw(&gpu, encoder, target, 257, 145, seconds, &camera, wind)
            })
        };
        let first = capture(wind, 0.0).unwrap();
        let later = capture(wind, 2.0).unwrap();
        let calm = SailWind { speed: 0.0, ..wind };
        let flat = capture(calm, 0.0).unwrap();
        let flat_later = capture(calm, 2.0).unwrap();
        // Compare only red cloth pixels: animated water cannot make this test pass.
        let mask = |frame: &Frame| {
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
        let red = mask(&first);
        for left in [true, false] {
            assert!(
                red.iter()
                    .enumerate()
                    .filter(|&(i, &p)| p > 0 && (i % 257 < 128) == left)
                    .count()
                    > 100,
                "both sail topologies must be visible"
            );
        }
        assert_ne!(
            red,
            mask(&later),
            "flutter must change cloth geometry/lighting"
        );
        assert_ne!(red, mask(&flat), "wind pressure must inflate the cloth");
        assert_eq!(mask(&flat), mask(&flat_later), "calm cloth must stay still");
        let invalid = SailWind {
            speed: f32::NAN,
            ..wind
        };
        assert!(capture(invalid, 0.0).is_err());
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn rigid_model_pose_moves_geometry_while_camera_and_water_stay_fixed() {
        use model::{LoadOptions, ModelAsset, ModelPose, ModelScene, OrbitCamera, SailWind};
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/modding-tool/Meshes/cannon_x.gm");
        let mut asset = ModelAsset::load(
            &path,
            &LoadOptions {
                untextured: true,
                ..Default::default()
            },
        )
        .unwrap();
        asset.textures[0].mips[0].rgba = vec![255, 0, 0, 255];
        let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
        let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
        let mut camera = OrbitCamera::new(&asset);
        camera.yaw = 0.0;
        let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &asset).unwrap();
        let mut capture = |pose| {
            scene.set_pose(pose)?;
            render_offscreen_with(&gpu, 257, 145, 0.0, |encoder, target| {
                scene.draw(
                    &gpu,
                    encoder,
                    target,
                    257,
                    145,
                    0.0,
                    &camera,
                    SailWind::default(),
                )
            })
        };
        let at_rest = capture(ModelPose::default()).unwrap();
        let moved = capture(ModelPose {
            translation: [0.25, 0.0, 0.0],
            ..Default::default()
        })
        .unwrap();
        let turned = capture(ModelPose {
            yaw: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        })
        .unwrap();
        let centroid = |f: &Frame| {
            let xs = f
                .rgba
                .chunks_exact(4)
                .enumerate()
                .filter(|(_, p)| p[0] > 100 && p[1] < 20 && p[2] < 20)
                .map(|(i, _)| (i % 257) as f64)
                .collect::<Vec<_>>();
            assert!(xs.len() > 50);
            xs.iter().sum::<f64>() / xs.len() as f64
        };
        assert!(centroid(&moved) > centroid(&at_rest) + 10.0);
        assert_ne!(at_rest.rgba, turned.rgba);
        for pose in [
            ModelPose {
                translation: [0.0, 0.15, 0.0],
                ..Default::default()
            },
            ModelPose {
                pitch: 0.2,
                ..Default::default()
            },
            ModelPose {
                roll: 0.3,
                ..Default::default()
            },
        ] {
            assert_ne!(at_rest.rgba, capture(pose).unwrap().rgba);
        }
        assert_eq!(
            &at_rest.rgba[..257 * 4],
            &moved.rgba[..257 * 4],
            "pose must not move the sky"
        );
        assert!(
            capture(ModelPose {
                yaw: f32::NAN,
                ..Default::default()
            })
            .is_err()
        );
    }
}
