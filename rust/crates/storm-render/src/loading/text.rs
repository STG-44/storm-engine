use anyhow::{Result, anyhow, ensure};
use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle},
};

const MAX_BITMAP_SIDE: usize = 2048;
const PADDING: [usize; 2] = [18, 12];

pub struct LoadingText {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) coverage: Vec<u8>,
}

impl LoadingText {
    pub fn rasterize(font: &[u8], message: &str, scale: f32) -> Result<Option<Self>> {
        if message.trim().is_empty() {
            return Ok(None);
        }
        ensure!(message.len() <= 4096, "loading message exceeds 4096 bytes");
        let font = Font::from_bytes(font, FontSettings::default())
            .map_err(|error| anyhow!("invalid loading font: {error}"))?;
        let scale = if scale.is_finite() {
            scale.clamp(0.5, 3.0)
        } else {
            1.0
        };
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            max_width: Some(680.0),
            horizontal_align: HorizontalAlign::Center,
            ..LayoutSettings::default()
        });
        layout.append(
            std::slice::from_ref(&font),
            &TextStyle::new(message, 24.0 * scale, 0),
        );
        let glyphs = layout
            .glyphs()
            .iter()
            .filter(|glyph| glyph.width != 0 && glyph.height != 0);
        let bounds =
            glyphs
                .clone()
                .try_fold(None, |bounds, glyph| -> Result<Option<[i64; 4]>> {
                    ensure!(
                        glyph.width <= MAX_BITMAP_SIDE
                            && glyph.height <= MAX_BITMAP_SIDE
                            && glyph.x.abs() <= MAX_BITMAP_SIDE as f32
                            && glyph.y.abs() <= MAX_BITMAP_SIDE as f32,
                        "loading message exceeds texture bounds"
                    );
                    let [x, y] = [glyph.x.floor() as i64, glyph.y.floor() as i64];
                    let [right, bottom] = [x + glyph.width as i64, y + glyph.height as i64];
                    Ok(Some(match bounds {
                        Some([left, top, old_right, old_bottom]) => [
                            left.min(x),
                            top.min(y),
                            old_right.max(right),
                            old_bottom.max(bottom),
                        ],
                        None => [x, y, right, bottom],
                    }))
                })?;
        let Some([left, top, right, bottom]) = bounds else {
            return Ok(None);
        };
        let width = (right - left) as usize + PADDING[0] * 2;
        let height = (bottom - top) as usize + PADDING[1] * 2;
        ensure!(
            width <= MAX_BITMAP_SIDE && height <= MAX_BITMAP_SIDE,
            "loading message exceeds texture bounds"
        );
        let mut coverage = vec![0u8; width * height];
        for glyph in glyphs {
            let (metrics, pixels) = font.rasterize_config(glyph.key);
            ensure!(
                metrics.width == glyph.width && metrics.height == glyph.height,
                "loading glyph metrics changed during rasterization"
            );
            let x = (glyph.x.floor() as i64 - left) as usize + PADDING[0];
            let y = (glyph.y.floor() as i64 - top) as usize + PADDING[1];
            for (row, source) in pixels.chunks_exact(glyph.width).enumerate() {
                let first = (y + row) * width + x;
                for (out, sample) in coverage[first..first + glyph.width].iter_mut().zip(source) {
                    *out = (*out).max(*sample);
                }
            }
        }
        Ok(Some(Self {
            width,
            height,
            coverage,
        }))
    }
}
