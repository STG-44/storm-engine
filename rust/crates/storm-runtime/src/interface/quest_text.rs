use crate::{GameDirectory, ScriptEncoding};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct QuestText {
    files: BTreeSet<String>,
    quests: BTreeMap<String, Quest>,
}
struct Quest {
    title: String,
    texts: BTreeMap<String, String>,
}

impl QuestText {
    pub fn load(&mut self, game: &GameDirectory, file: &str) -> Result<()> {
        if file.is_empty() || !self.files.insert(file.to_owned()) {
            return Ok(());
        }
        let Some(bytes) = game.read_optional(file, 8 * 1024 * 1024)? else {
            eprintln!("XINTERFACE: quest log file not found: {file}");
            return Ok(());
        };
        let source = ScriptEncoding::Utf8
            .decode(&bytes)
            .or_else(|_| ScriptEncoding::Windows1251.decode(&bytes))?;
        let mut current = "";
        for token in source.split('#').skip(1) {
            let Some((kind, rest)) = token.split_once(|c: char| c.is_ascii_whitespace()) else {
                continue;
            };
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
            let (id, text) = rest
                .split_once(|c: char| c.is_ascii_whitespace())
                .unwrap_or((rest, ""));
            let text = text.trim_ascii();
            if id.is_empty() {
                continue;
            }
            match kind {
                "QUEST" => {
                    current = id;
                    self.quests.entry(id.to_owned()).or_insert_with(|| Quest {
                        title: text.to_owned(),
                        texts: BTreeMap::new(),
                    });
                }
                "TEXT" => {
                    if let Some(quest) = self.quests.get_mut(current) {
                        quest
                            .texts
                            .entry(id.to_owned())
                            .or_insert_with(|| text.to_owned());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn title(&self, id: &str) -> Option<&str> {
        self.quests.get(id).map(|quest| quest.title.as_str())
    }

    pub fn text(&self, id: &str, entry: &str) -> Option<&str> {
        self.quests.get(id)?.texts.get(entry).map(String::as_str)
    }
}
