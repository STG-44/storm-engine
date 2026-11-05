use crate::{
    GameDirectory,
    config::{configuration_text, root_fields},
};
use anyhow::{Context, Result, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowSettings {
    pub width: u32,
    pub height: u32,
    pub display: usize,
    pub fullscreen: bool,
    pub borders: bool,
    pub run_in_background: bool,
    pub sound_in_background: bool,
    pub max_fps: u32,
    pub vsync: bool,
}
fn integer(text: &str) -> Result<i32> {
    // IFS::GetInt uses atoll followed by an int32 cast. Preserve its decimal
    // prefix/sign behavior and bit narrowing; reject undefined atoll overflow.
    let text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let sign = usize::from(text.starts_with(['+', '-']));
    let end = sign
        + text.as_bytes()[sign..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    if end == sign {
        return Ok(0);
    }
    Ok(text[..end]
        .parse::<i64>()
        .context("engine.ini integer overflow")? as i32)
}
impl WindowSettings {
    pub fn parse(text: &str) -> Result<Self> {
        let fields = root_fields(text)?;
        let get = |name: &str, default: i32| -> Result<i32> {
            fields
                .get(name)
                .and_then(|s| s.as_deref())
                .map(integer)
                .transpose()
                .with_context(|| format!("engine.ini {name}"))
                .map(|n| n.unwrap_or(default))
        };
        let width = get("screen_x", 1024)?;
        let height = get("screen_y", 768)?;
        ensure!(
            (1..=16384).contains(&width) && (1..=16384).contains(&height),
            "configured window dimensions must be 1..16384"
        );
        let display = get("display", 0)?;
        ensure!(display >= 0, "configured display must not be negative");
        let run_in_background = get("run_in_background", 0)? != 0;
        Ok(Self {
            width: width as u32,
            height: height as u32,
            display: display as usize,
            fullscreen: get("full_screen", 0)? != 0,
            borders: get("window_borders", 0)? != 0,
            run_in_background,
            sound_in_background: run_in_background && get("sound_in_background", 1)? != 0,
            max_fps: get("max_fps", 0)? as u32,
            vsync: get("vsync", 0)? != 0,
        })
    }
    pub fn load(game: &GameDirectory) -> Result<Self> {
        Self::parse(&configuration_text(game)?)
    }
    /// Original main loop uses integer millisecond division, including zero
    /// when max_fps exceeds 1000. Presentation may impose a platform fallback.
    pub fn minimum_frame_ms(&self) -> u32 {
        1000u32.checked_div(self.max_fps).unwrap_or(0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_window_defaults_root_duplicates_and_decimal_prefixes_are_retained() {
        let s = WindowSettings::parse(
            "screen_x\nscreen_x=900\nscreen_y=\nscreen_y=700\n[video]\nfull_screen=1",
        )
        .unwrap();
        assert_eq!((s.width, s.height, s.display), (1024, 768, 0));
        assert!(
            !s.fullscreen
                && !s.borders
                && !s.run_in_background
                && !s.sound_in_background
                && !s.vsync
        );
        assert_eq!(s.minimum_frame_ms(), 0);
        let s=WindowSettings::parse("SCREEN_X=800px ;literal\nscreen_y=+600\nwindow_borders=-2\ndisplay=+1\nrun_in_background=-1\nmax_fps=120\nvsync=2").unwrap();
        assert_eq!((s.width, s.height, s.display), (800, 600, 1));
        assert!(s.borders && s.run_in_background && s.sound_in_background && s.vsync);
        assert_eq!(s.minimum_frame_ms(), 8);
        let s = WindowSettings::parse("run_in_background=1\nsound_in_background=0\nmax_fps=-1")
            .unwrap();
        assert!(!s.sound_in_background);
        assert_eq!(s.max_fps, u32::MAX);
        assert_eq!(s.minimum_frame_ms(), 0);
    }
    #[test]
    fn unsupported_window_values_and_integer_overflow_fail_with_config_context() {
        for text in [
            "screen_x=0",
            "screen_y=-1",
            "display=-1",
            "screen_x=99999999999999999999",
            "[broken",
        ] {
            assert!(WindowSettings::parse(text).is_err(), "{text}");
        }
        assert!(crate::Startup::parse("run\nrun=main.c").is_err());
    }
}
