use anyhow::Result;
use std::{
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use storm_formats::texture::Texture;
use storm_render::loading::LoadingPictures;

const UPDATE_INTERVAL: Duration = Duration::from_millis(50);
const CYCLE_STEPS: u8 = 32;

#[derive(Clone)]
pub struct LoadingProgress {
    pub pictures: Option<Rc<LoadingPictures>>,
    /// A repeating animation phase in [0, 1).
    pub phase: f32,
}

type Presenter = dyn FnMut(&LoadingProgress) -> Result<()>;

#[derive(Default)]
pub(super) struct Progress {
    active: Option<Active>,
    selection: PictureSelection,
    presenter: Option<Box<Presenter>>,
}

#[derive(Default)]
pub(super) struct PictureSelection {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub border: BorderSelection,
}

#[derive(Default)]
pub(super) enum BorderSelection {
    #[default]
    Inherit,
    Selected(Option<Arc<Texture>>),
}

#[derive(Clone, Copy)]
pub(super) enum PictureLayer {
    Background,
    Foreground,
}

struct Active {
    pictures: Option<Rc<LoadingPictures>>,
    last_update: Instant,
    step: u8,
}

impl Progress {
    pub fn set_picture(&mut self, layer: PictureLayer, name: String) {
        match layer {
            PictureLayer::Background => self.selection.background = Some(name),
            PictureLayer::Foreground => self.selection.foreground = Some(name),
        }
    }

    pub fn set_border(&mut self, border: Option<Arc<Texture>>) {
        self.selection.border = BorderSelection::Selected(border);
    }

    pub fn border(&self) -> Option<Arc<Texture>> {
        match &self.selection.border {
            BorderSelection::Inherit => None,
            BorderSelection::Selected(border) => border.clone(),
        }
    }

    pub fn start(
        &mut self,
        now: Instant,
        prepare_pictures: impl FnOnce(&PictureSelection) -> Result<Option<Rc<LoadingPictures>>>,
    ) -> Result<()> {
        if self.active.is_none() {
            self.active = Some(Active {
                pictures: prepare_pictures(&self.selection)?,
                last_update: now,
                step: 0,
            });
            self.present()?;
        }
        Ok(())
    }

    pub fn update(&mut self, now: Instant) -> Result<()> {
        if let Some(active) = &mut self.active
            && now.saturating_duration_since(active.last_update) >= UPDATE_INTERVAL
        {
            active.last_update = now;
            active.step = (active.step + 1) % CYCLE_STEPS;
            self.present()?;
        }
        Ok(())
    }

    pub fn end(&mut self) {
        self.active = None;
        self.selection = PictureSelection::default();
    }

    pub fn view(&self) -> Option<LoadingProgress> {
        let active = self.active.as_ref()?;
        Some(LoadingProgress {
            pictures: active.pictures.clone(),
            phase: f32::from(active.step) / f32::from(CYCLE_STEPS),
        })
    }

    pub fn set_presenter(
        &mut self,
        presenter: impl FnMut(&LoadingProgress) -> Result<()> + 'static,
    ) {
        self.presenter = Some(Box::new(presenter));
    }

    fn present(&mut self) -> Result<()> {
        if let Some(view) = self.view()
            && let Some(presenter) = &mut self.presenter
        {
            presenter(&view)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn progress_updates_are_throttled_and_restart_from_the_first_phase() -> Result<()> {
        let phases = Rc::new(RefCell::new(Vec::new()));
        let recorded = phases.clone();
        let start = Instant::now();
        let mut progress = Progress::default();
        progress.set_presenter(move |view| {
            recorded.borrow_mut().push(view.phase);
            Ok(())
        });
        progress.update(start)?;
        progress.start(start, |_| Ok(None))?;
        progress.start(start, |_| panic!("duplicate start reloads the pictures"))?;
        progress.update(start + Duration::from_millis(49))?;
        assert_eq!(&*phases.borrow(), &[0.0]);
        for step in 1..=CYCLE_STEPS {
            progress.update(start + UPDATE_INTERVAL * u32::from(step))?;
        }
        assert_eq!(phases.borrow().len(), 33);
        assert_eq!(phases.borrow()[1], 1.0 / 32.0);
        assert_eq!(phases.borrow()[32], 0.0);
        progress.end();
        progress.end();
        assert!(progress.view().is_none());
        progress.start(start, |_| Ok(None))?;
        assert_eq!(progress.view().unwrap().phase, 0.0);
        Ok(())
    }
}
