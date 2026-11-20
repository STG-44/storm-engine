//! Shared scene and transition rendering for the window and PNG output.
use anyhow::Result;
use std::rc::Rc;
use storm_core::EntityId;
use storm_render::{
    Gpu,
    color::{ColorCorrection, ColorPass},
    fade::{FadeCompositor, FadeDraw},
    loading::{LoadingPictures, LoadingScreen},
};
use storm_runtime::{
    interface::LoadingProgress,
    model::{Draw, ModelScenes},
};

pub(crate) struct SceneFrame<'a> {
    pub draws: &'a [Draw],
    pub live: &'a [EntityId],
    pub seconds: f32,
    pub pictures: Option<Rc<LoadingPictures>>,
    pub fades: &'a [FadeDraw],
    pub correction: Option<ColorCorrection>,
}

pub(crate) struct SceneRenderer {
    format: wgpu::TextureFormat,
    models: ModelScenes,
    loading: Option<(Option<Rc<LoadingPictures>>, LoadingScreen)>,
    fade: FadeCompositor,
    color: ColorPass,
}

impl SceneRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            models: ModelScenes::new(format),
            loading: None,
            fade: FadeCompositor::new(gpu, format),
            color: ColorPass::new(format),
        }
    }

    fn prepare_loading(&mut self, gpu: &Gpu, pictures: Option<Rc<LoadingPictures>>) -> Result<()> {
        let unchanged =
            self.loading
                .as_ref()
                .is_some_and(|(cached, _)| match (cached, &pictures) {
                    (Some(old), Some(new)) => Rc::ptr_eq(old, new),
                    (None, None) => true,
                    _ => false,
                });
        if !unchanged {
            let screen = LoadingScreen::new(
                gpu,
                self.format,
                pictures.as_deref().unwrap_or(&LoadingPictures::default()),
            )?;
            self.loading = Some((pictures, screen));
        }
        Ok(())
    }

    pub fn prune(&mut self, live: &[EntityId]) {
        self.models.prune(live);
    }

    pub fn needs_capture(&self, fades: &[FadeDraw]) -> bool {
        self.fade.needs_capture(fades)
    }

    pub fn capture(&mut self, gpu: &Gpu, size: [u32; 2], frame: &SceneFrame<'_>) -> Result<()> {
        if self.needs_capture(frame.fades) {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            self.encode(gpu, &mut encoder, None, size, frame)?;
            gpu.queue.submit([encoder.finish()]);
        }
        Ok(())
    }

    pub fn encode(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: Option<&wgpu::TextureView>,
        [width, height]: [u32; 2],
        frame: &SceneFrame<'_>,
    ) -> Result<()> {
        self.models.prepare(gpu, frame.draws, frame.live)?;
        if let Some(pictures) = &frame.pictures {
            self.prepare_loading(gpu, Some(pictures.clone()))?;
        } else {
            self.loading = None;
        }
        let uncorrected = target
            .zip(frame.correction)
            .map(|_| self.color.source(gpu, [width, height]));
        self.fade.encode_scene(
            gpu,
            encoder,
            uncorrected.as_ref().or(target),
            [width, height],
            frame.fades,
            |encoder, view| {
                self.models
                    .encode(gpu, encoder, view, width, height, frame.seconds)?;
                if let Some((_, screen)) = &self.loading {
                    screen.encode(gpu, encoder, view, [width, height]);
                }
                Ok(())
            },
        )?;
        if let (Some(source), Some(target), Some(correction)) =
            (&uncorrected, target, frame.correction)
        {
            self.color.encode(gpu, encoder, source, target, correction);
        }
        self.models.prune(frame.live);
        Ok(())
    }

    pub fn encode_progress(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        progress: &LoadingProgress,
        correction: Option<ColorCorrection>,
    ) -> Result<()> {
        self.prepare_loading(gpu, progress.pictures.clone())?;
        let (_, screen) = self.loading.as_ref().expect("loading screen prepared");
        if let Some(correction) = correction {
            let source = self.color.source(gpu, size);
            screen.encode_progress(gpu, encoder, &source, size, progress.phase);
            self.color.encode(gpu, encoder, &source, target, correction);
        } else {
            screen.encode_progress(gpu, encoder, target, size, progress.phase);
        }
        Ok(())
    }
}
