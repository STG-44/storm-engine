use super::{Shared, load_texture};
use anyhow::Result;
use std::rc::Rc;
use storm_core::{Entity, EntityInit, Message, Stage};
use storm_render::loading::{LoadingPictures, LoadingText};

pub(super) struct InfoActor {
    shared: Rc<Shared>,
    pictures: Option<Rc<LoadingPictures>>,
}

impl InfoActor {
    pub fn new(shared: Rc<Shared>) -> Self {
        Self {
            shared,
            pictures: None,
        }
    }

    fn draw(&self) {
        if let Some(pictures) = &self.pictures {
            self.shared.loading_draw.replace(Some(pictures.clone()));
        }
    }
}

impl Entity for InfoActor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        let Some(root) = context.attributes else {
            return Ok(false);
        };
        let texture = |key| {
            context
                .store
                .value(root, Some(key))
                .filter(|name| !name.is_empty())
                .map(|name| load_texture(&self.shared, name))
                .transpose()
                .map(Option::flatten)
        };
        let background = texture("picbackfilename")?;
        let foreground = texture("picfilename")?;
        let text = if let Some(message) = context.store.value(root, Some("infoStr"))
            && !message.trim().is_empty()
        {
            let name = context
                .store
                .value(root, Some("fontfilename"))
                .filter(|value| !value.is_empty())
                .unwrap_or("RESOURCE/Fonts/Interface.ttf");
            match self.shared.game.read_optional(name, 8 * 1024 * 1024)? {
                Some(bytes) => LoadingText::rasterize(
                    &bytes,
                    message,
                    context.store.as_float(root, Some("scale"), 1.0),
                )?,
                None => None,
            }
        } else {
            None
        };
        if background.is_some() || foreground.is_some() || text.is_some() {
            let border = if foreground.is_some() {
                texture("picborderfilename")?
            } else {
                None
            };
            self.pictures = Some(Rc::new(LoadingPictures {
                background,
                foreground,
                border,
                text,
            }));
        }
        Ok(true)
    }

    fn process_stage(&mut self, stage: Stage, _: u32) {
        if stage == Stage::Realize {
            self.draw();
        }
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if message.long()? == 1 {
            self.draw();
        }
        Ok(0)
    }
}
