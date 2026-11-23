use super::*;
use std::collections::BTreeSet;
use storm_render::{
    Frame, Gpu,
    model::{
        AstronomyRenderer, Atmosphere, CelestialDraw, ModelScene, OceanRenderer, OrbitCamera,
        RainDraw, RainRenderer, SailWind, SkyDraw, SkyRenderer, SunDraw, SunRenderer,
        native_model_matrix,
    },
};

/// GPU scenes follow the registry's draw order and asset revision. This bounded
/// static path reuses the existing shader/material foundation.
pub struct ModelScenes {
    owners: Vec<Owner>,
    scenes: Vec<ModelScene>,
    active: usize,
    camera: OrbitCamera,
    framed: bool,
    format: wgpu::TextureFormat,
    ocean: Option<OceanRenderer>,
    rain: Option<RainRenderer>,
    sun: Option<SunRenderer>,
    astronomy: Option<AstronomyRenderer>,
    sky: Option<SkyRenderer>,
}
#[derive(Default)]
pub struct AtmosphereDraws<'a> {
    pub rain: &'a [RainDraw],
    pub sun: &'a [SunDraw],
    pub astronomy: &'a [CelestialDraw],
    pub sky: Option<&'a SkyDraw>,
}
struct Owner {
    id: u64,
    revision: u64,
    asset: std::rc::Weak<ModelAsset>,
}
impl Owner {
    fn for_draw(draw: &Draw) -> Self {
        Self {
            id: draw.entity.raw(),
            revision: draw.revision,
            asset: Rc::downgrade(&draw.asset),
        }
    }
    fn matches(&self, draw: &Draw) -> bool {
        self.id == draw.entity.raw()
            && self.revision == draw.revision
            && self
                .asset
                .upgrade()
                .is_some_and(|asset| Rc::ptr_eq(&asset, &draw.asset))
    }
}
impl Default for ModelScenes {
    fn default() -> Self {
        Self::new(wgpu::TextureFormat::Rgba8UnormSrgb)
    }
}
impl ModelScenes {
    pub fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            owners: Vec::new(),
            scenes: Vec::new(),
            active: 0,
            camera: OrbitCamera::from_bounds([0.0; 3].into(), 1.0),
            framed: false,
            format,
            ocean: None,
            rain: None,
            sun: None,
            astronomy: None,
            sky: None,
        }
    }
    pub fn cached(&self) -> usize {
        self.scenes.len()
    }
    pub fn camera(&self) -> &OrbitCamera {
        &self.camera
    }
    pub fn set_camera(&mut self, camera: OrbitCamera) {
        self.camera = camera;
        self.framed = true;
    }
    pub fn prepare(&mut self, gpu: &Gpu, draws: &[Draw], live: &[EntityId]) -> Result<()> {
        ensure!(
            draws.len() <= 16,
            "runtime renderer supports at most 16 static model draws"
        );
        let mut ids = BTreeSet::new();
        let mut replacements = BTreeMap::new();
        for draw in draws {
            ensure!(
                ids.insert(draw.entity.raw()),
                "repeated model draw owners require per-draw uniform storage"
            );
            native_model_matrix(draw.matrix)?;
        }
        // Validate inputs and build replacement storage before moving existing
        // scenes. Device loss/OOM are outside this checked setup boundary.
        for draw in draws {
            if !self.owners.iter().any(|owner| owner.matches(draw)) {
                let mut scene = ModelScene::new(gpu, self.format, &draw.asset)?;
                scene.set_native_model_matrix(Some(draw.matrix))?;
                replacements.insert(draw.entity.raw(), scene);
            }
        }
        let mut previous = std::mem::take(&mut self.owners)
            .into_iter()
            .zip(std::mem::take(&mut self.scenes))
            .map(|(owner, scene)| (owner.id, (owner, scene)))
            .collect::<BTreeMap<_, _>>();
        let mut scenes = Vec::new();
        let mut owners = Vec::new();
        for draw in draws {
            let mut scene = if let Some(scene) = replacements.remove(&draw.entity.raw()) {
                scene
            } else {
                previous
                    .remove(&draw.entity.raw())
                    .context("missing retained GPU model")?
                    .1
            };
            scene.set_native_model_matrix(Some(draw.matrix))?;
            scene.set_ambient_light(draw.ambient_light);
            scenes.push(scene);
            owners.push(Owner::for_draw(draw));
        }
        self.active = scenes.len();
        let live = live.iter().map(|id| id.raw()).collect::<BTreeSet<_>>();
        for (id, (owner, scene)) in previous {
            if live.contains(&id) && !ids.contains(&id) && scenes.len() < 16 {
                owners.push(owner);
                scenes.push(scene);
            }
        }
        self.scenes = scenes;
        self.owners = owners;
        if !self.framed && !draws.is_empty() {
            let mut bounds: Option<([f32; 3], [f32; 3])> = None;
            for draw in draws {
                let matrix = native_model_matrix(draw.matrix)?;
                let center = matrix.transform_point3(draw.asset.center).to_array();
                let scale = matrix
                    .x_axis
                    .truncate()
                    .length()
                    .max(matrix.y_axis.truncate().length())
                    .max(matrix.z_axis.truncate().length());
                let radius = draw.asset.radius * scale;
                let lo = std::array::from_fn(|i| center[i] - radius);
                let hi = std::array::from_fn(|i| center[i] + radius);
                bounds = Some(if let Some((a, b)) = bounds {
                    (
                        std::array::from_fn(|i| a[i].min(lo[i])),
                        std::array::from_fn(|i| b[i].max(hi[i])),
                    )
                } else {
                    (lo, hi)
                });
            }
            let (lo, hi) = bounds.unwrap();
            let center = std::array::from_fn::<_, 3, _>(|i| (lo[i] + hi[i]) * 0.5);
            let radius = (0..3)
                .map(|i| ((hi[i] - lo[i]) * 0.5).powi(2))
                .sum::<f32>()
                .sqrt()
                .max(0.1);
            self.camera = OrbitCamera::from_bounds(center.into(), radius);
            self.framed = true;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        seconds: f32,
    ) -> Result<()> {
        ModelScene::draw_models(
            &mut self.scenes[..self.active],
            gpu,
            encoder,
            target,
            width,
            height,
            seconds,
            &self.camera,
            SailWind {
                speed: 0.0,
                ..Default::default()
            },
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode_with_sea(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        seconds: f32,
        surface: &crate::sea::Surface,
    ) -> Result<()> {
        self.encode_weather(gpu, encoder, target, size, seconds, Some(surface), &[])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_weather(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        seconds: f32,
        sea: Option<&crate::sea::Surface>,
        rain: &[RainDraw],
    ) -> Result<()> {
        self.encode_atmosphere(gpu, encoder, target, size, seconds, sea, rain, &[])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_atmosphere(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        seconds: f32,
        sea: Option<&crate::sea::Surface>,
        rain: &[RainDraw],
        sun: &[SunDraw],
    ) -> Result<()> {
        self.encode_configured(
            gpu,
            encoder,
            target,
            size,
            seconds,
            sea,
            AtmosphereDraws {
                rain,
                sun,
                ..AtmosphereDraws::default()
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_configured(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        seconds: f32,
        sea: Option<&crate::sea::Surface>,
        AtmosphereDraws {
            rain,
            sun,
            astronomy,
            sky,
        }: AtmosphereDraws<'_>,
    ) -> Result<()> {
        let water_view = sea.filter(|_| !self.framed).map(|surface| {
            // The model-independent orbit keeps the eye above the wave envelope
            // until an actual scene selects its camera.
            let wave_height = surface
                .parameters
                .amplitude
                .iter()
                .map(|v| v.abs())
                .sum::<f32>();
            OrbitCamera::from_bounds([0.0, wave_height, 0.0].into(), wave_height.max(1.0) * 4.0)
        });
        let ocean = sea.map(|surface| {
            (
                self.ocean
                    .get_or_insert_with(|| OceanRenderer::new(gpu, self.format)),
                surface,
            )
        });
        let rain = (!rain.is_empty()).then(|| {
            (
                self.rain
                    .get_or_insert_with(|| RainRenderer::new(gpu, self.format)),
                rain,
            )
        });
        let sun = (!sun.is_empty()).then(|| {
            (
                self.sun
                    .get_or_insert_with(|| SunRenderer::new(gpu, self.format)),
                sun,
            )
        });
        let astronomy = (!astronomy.is_empty()).then(|| {
            (
                self.astronomy
                    .get_or_insert_with(|| AstronomyRenderer::new(gpu, self.format)),
                astronomy,
            )
        });
        let sky = sky.map(|draw| {
            (
                self.sky
                    .get_or_insert_with(|| SkyRenderer::new(gpu, self.format)),
                draw,
            )
        });
        ModelScene::draw_configured_with_atmosphere(
            &mut self.scenes[..self.active],
            gpu,
            encoder,
            target,
            size,
            seconds,
            water_view.as_ref().unwrap_or(&self.camera),
            ocean,
            Atmosphere {
                rain,
                sun,
                astronomy,
                sky,
            },
        )
    }
    pub fn offscreen(&mut self, gpu: &Gpu, width: u32, height: u32, seconds: f32) -> Result<Frame> {
        ensure!(
            self.format == wgpu::TextureFormat::Rgba8UnormSrgb,
            "offscreen model scenes require RGBA8 sRGB storage"
        );
        storm_render::render_models_offscreen(
            gpu,
            &mut self.scenes[..self.active],
            &self.camera,
            width,
            height,
            seconds,
        )
    }
    /// Call after encoding/submission to release even a model which was erased
    /// after its Realize request. Submitted wgpu commands retain their resources.
    pub fn prune(&mut self, live: &[EntityId]) {
        let live = live.iter().map(|id| id.raw()).collect::<BTreeSet<_>>();
        let mut owners = Vec::new();
        let mut scenes = Vec::new();
        let mut active = 0;
        for (index, (owner, scene)) in std::mem::take(&mut self.owners)
            .into_iter()
            .zip(std::mem::take(&mut self.scenes))
            .enumerate()
        {
            if live.contains(&owner.id) {
                if index < self.active {
                    active += 1;
                }
                owners.push(owner);
                scenes.push(scene);
            }
        }
        self.owners = owners;
        self.scenes = scenes;
        self.active = active;
    }
}
