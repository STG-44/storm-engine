use super::{
    FULL_UV, NEUTRAL, NodeEvents, Nodes, Setting, Shared,
    button::{ACTIVATE, Action, CLICK, DOUBLE_CLICK, RIGHT_CLICK},
    color, flag, ini_color, ini_integer, inside, load_font, solid_mask, tuple,
};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_core::{AttributeId, AttributeStore, Message, Value};
use storm_formats::texture::Texture;
use storm_render::interface_text::{
    Alignment, BitmapFont, InterfaceDraw, PictureColor, PictureDraw, TextDraw,
};

const MAX_AXES: usize = 64;
const MAX_ROWS: usize = 4096;
const MAX_CELLS: usize = 32_768;
const MAX_TEXT_BYTES: usize = 4096;

pub(super) struct Table {
    columns: Vec<i32>,
    heights: Vec<i32>,
    grid: Grid,
    back: Option<super::image::InterfaceImage>,
    highlight: Option<super::image::InterfaceImage>,
    mask: Rc<Texture>,
    body_style: FontStyle,
    header_style: FontStyle,
    fonts: Vec<Option<Rc<BitmapFont>>>,
    select_columns: bool,
    top: usize,
    selected: Option<usize>,
    col: Option<usize>,
    header: Option<Row>,
    rows: Vec<Row>,
    seen: bool,
    selection_dirty: bool,
    top_dirty: bool,
}

#[derive(Clone, Copy)]
enum Vertical {
    Top,
    Center,
    Bottom,
}

struct FontStyle {
    font: Option<Rc<BitmapFont>>,
    color: [u8; 4],
    scale: f32,
    horizontal: Alignment,
    vertical: Vertical,
}

struct Row {
    color: [u8; 4],
    cells: Vec<Cell>,
}

struct Cell {
    font: Option<Rc<BitmapFont>>,
    color: [u8; 4],
    scale: f32,
    offset: [f32; 2],
    line_height: f32,
    advance: f32,
    vertical: Vertical,
    lines: Vec<(String, i32, Option<f32>)>,
}

struct Grid {
    texture: Option<Rc<Texture>>,
    uv: [Option<[f32; 4]>; 10],
    rgba: [u8; 4],
    border: i32,
    vertical: i32,
    horizontal: i32,
    header: i32,
    segmented: bool,
    corner: [i32; 2],
    padding: [i32; 2],
}

impl Table {
    pub(super) fn load(
        nodes: &mut Nodes,
        shared: &Shared,
        setting: &Setting<'_>,
        rect: &mut [i32; 4],
    ) -> Result<Self> {
        ensure!(
            !flag(setting("IsVariableLineHeight"), false),
            "unported XINTERFACE variable-height table"
        );
        let grid = Grid::load(nodes, shared, setting)?;
        let width = rect[2].saturating_sub(rect[0]);
        let height = rect[3].saturating_sub(rect[1]);
        let columns = dimensions(
            setting("colswidth"),
            setting("colquantity"),
            width.saturating_sub(grid.border),
        )?;
        let heights = dimensions(setting("rowsheight"), setting("rowquantity"), height)?;
        rect[2] = edge(rect[0], &columns).saturating_add(grid.border);
        rect[3] = edge(rect[1], &heights).saturating_add(grid.border);

        let mut fonts = Vec::new();
        for i in 1..=100 {
            let Some(name) = setting(&format!("fontlist{i}")) else {
                break;
            };
            fonts.push(load_font(shared, name)?);
        }
        let mut image = |key| -> Result<_> {
            match setting(key) {
                Some(value) => super::image::InterfaceImage::load(nodes, shared, value),
                None => Ok(None),
            }
        };
        let select_columns = flag(setting("iscolselect"), false);
        Ok(Self {
            columns,
            heights,
            grid,
            back: image("backimage")?,
            highlight: image("selectimage")?,
            mask: solid_mask(),
            body_style: FontStyle::load(shared, setting, "cell")?,
            header_style: FontStyle::load(shared, setting, "title")?,
            fonts,
            select_columns,
            top: 0,
            selected: Some(0),
            col: select_columns.then_some(0),
            header: None,
            rows: Vec::new(),
            seen: false,
            selection_dirty: true,
            top_dirty: false,
        })
    }

    pub(super) fn refresh(
        &mut self,
        store: &AttributeStore,
        root: AttributeId,
        name: &str,
    ) -> Result<()> {
        let Some(field) = store.child(root, name) else {
            self.header = None;
            self.rows.clear();
            return Ok(());
        };
        let mut rows = Vec::new();
        for index in 1..=MAX_ROWS {
            let Some(id) = store.child(field, &format!("tr{index}")) else {
                break;
            };
            ensure!(
                index * self.columns.len() <= MAX_CELLS,
                "table cell count budget"
            );
            rows.push(self.read_row(store, id, false)?);
        }
        let header = store
            .child(field, "hr")
            .map(|id| self.read_row(store, id, true))
            .transpose()?;
        self.rows = rows;
        self.header = header;
        self.top = (store.as_dword(field, Some("top"), 0) as usize).min(self.rows.len());
        let current = self.selected.map_or(0, |index| index + 1) as u32;
        let selected = (store.as_dword(field, Some("select"), current) as usize)
            .min(self.rows.len())
            .checked_sub(1);
        self.selection_dirty |= self.selected != selected;
        self.selected = selected;
        Ok(())
    }

    fn read_row(&self, store: &AttributeStore, id: AttributeId, header: bool) -> Result<Row> {
        let mut cells = Vec::new();
        for column in 0..self.columns.len() {
            let Some(attr) = store.child(id, &format!("td{}", column + 1)) else {
                break;
            };
            let style = if header {
                &self.header_style
            } else {
                &self.body_style
            };
            let line = if header {
                self.grid.header
            } else {
                self.grid.horizontal
            };
            let margin = if column == 0 {
                self.grid.border
            } else {
                self.grid.vertical
            };
            let text = store.value(attr, Some("str")).unwrap_or_default();
            ensure!(text.len() <= MAX_TEXT_BYTES, "table cell text budget");
            let scale = store.as_float(attr, Some("scale"), style.scale);
            let spacing = store.as_float(attr, Some("line_space_modifier"), 1.0);
            let offset: [f32; 2] = store
                .value(attr, Some("textoffset"))
                .map_or(Ok([0.0; 2]), tuple)?;
            ensure!(
                scale.is_finite()
                    && scale >= 0.0
                    && spacing.is_finite()
                    && offset.iter().all(|p| p.is_finite()),
                "invalid table cell text geometry"
            );
            let font = self
                .fonts
                .get(store.as_dword(attr, Some("fontidx"), u32::MAX) as usize)
                .and_then(Clone::clone)
                .or_else(|| style.font.clone());
            let line_height = style
                .font
                .as_ref()
                .map_or(0.0, |font| (font.height as f32 * font.pc_scale).trunc())
                * scale;
            let width = self.columns[column]
                .saturating_sub(self.grid.padding[0].saturating_mul(2))
                .saturating_sub(margin)
                .saturating_sub(offset[0] as i32);
            let width = if width > 0 { width } else { 32 };
            let measure = |text: &str| {
                style
                    .font
                    .as_ref()
                    .map_or(0, |font| (font.text_width(text) as f32 * scale) as i32)
            };
            let horizontal = horizontal(store.value(attr, Some("align")), style.horizontal);
            let offsets = store.child(attr, "textoffsets");
            let lines = super::wrap::lines(text, width, measure)
                .into_iter()
                .enumerate()
                .map(|(index, text)| {
                    let text = text.trim_end().to_owned();
                    let remainder = width.saturating_sub(measure(&text));
                    let x = match horizontal {
                        Alignment::Left => 0,
                        Alignment::Center => remainder / 2,
                        Alignment::Right => remainder,
                    };
                    let y = offsets
                        .and_then(|id| store.value(id, Some(&format!("s{}", index + 1))))
                        .and_then(|v| v.parse::<f32>().ok())
                        .filter(|v| v.is_finite() && *v != -1000.0);
                    (text, margin.saturating_add(x), y)
                })
                .collect();
            let vertical = vertical(store.value(attr, Some("valign")), style.vertical);
            let x = offset[0] + self.grid.padding[0] as f32;
            let y = offset[1] + self.grid.padding[1] as f32 + line as f32;
            let color = store.value(attr, Some("color")).map_or(style.color, |_| {
                color(store.as_dword(attr, Some("color"), 0) as i32)
            });
            cells.push(Cell {
                font,
                color,
                scale,
                offset: [x, y],
                line_height,
                advance: line_height * spacing,
                lines,
                vertical,
            });
        }
        Ok(Row {
            color: color(store.as_dword(id, Some("speccolor"), 0) as i32),
            cells,
        })
    }

    pub(super) fn publish(
        &mut self,
        store: &mut AttributeStore,
        root: AttributeId,
        name: &str,
    ) -> Result<()> {
        if !self.selection_dirty && !self.top_dirty {
            return Ok(());
        }
        let node = match store.child(root, name) {
            Some(node) => node,
            None => store.create_attribute(root, name, None)?,
        };
        if self.selection_dirty {
            store.set_dword(
                node,
                Some("select"),
                self.selected.map_or(0, |n| n as u32 + 1),
            )?;
            store.set_dword(
                node,
                Some("selectcol"),
                self.col.map_or(0, |n| n as u32 + 1),
            )?;
            self.selection_dirty = false;
        }
        if self.top_dirty {
            store.set_dword(node, Some("top"), self.top as u32)?;
            self.top_dirty = false;
        }
        Ok(())
    }

    pub(super) fn row_at(&self, rect: [i32; 4], point: Option<[f64; 2]>) -> Option<usize> {
        let point = point.filter(|&point| inside(rect, point))?;
        let mut bottom = rect[1];
        self.heights.iter().position(|&height| {
            bottom = bottom.saturating_add(height);
            point[1] < f64::from(bottom)
        })
    }

    pub(super) fn execute(
        &mut self,
        rect: [i32; 4],
        point: Option<[f64; 2]>,
        name: &str,
        action: Action,
        events: &mut NodeEvents,
    ) -> Result<()> {
        let selected = self.selected.map_or(-1, |row| row as i32);
        if matches!(action, ACTIVATE | DOUBLE_CLICK) && selected >= 0 {
            return event(events, "TableActivate", name, selected, None);
        }
        let mut click_col = None;
        let head = usize::from(self.header.is_some());
        let page = self.heights.len().saturating_sub(head).max(1);
        if matches!(action, CLICK | RIGHT_CLICK) {
            let Some(visible_row) = self
                .row_at(rect, point)
                .and_then(|row| row.checked_sub(head))
            else {
                return Ok(());
            };
            let row = self.top + visible_row;
            if row >= self.rows.len() {
                return Ok(());
            }
            let mut right = rect[0];
            let col = self.columns.iter().position(|&width| {
                right = right.saturating_add(width);
                point.is_some_and(|[x, _]| x <= f64::from(right))
            });
            if self.select_columns {
                self.col = col;
            }
            self.selected = Some(row);
            if action == CLICK {
                click_col = Some(col.map_or(0, |n| n as i32 + 1));
            }
        } else {
            match action.0 {
                3 | 4 | 10 | 11 if !self.rows.is_empty() => {
                    let direction = if action.0 == 3 || action.0 == 10 {
                        -1
                    } else {
                        1
                    };
                    if direction < 0 && selected <= 0 {
                        return Ok(());
                    }
                    let step = if action.0 == 10 || action.0 == 11 {
                        page as i32
                    } else {
                        1
                    };
                    let next =
                        (selected + direction * step).clamp(0, self.rows.len() as i32 - 1) as usize;
                    if self.selected == Some(next) {
                        return Ok(());
                    }
                    self.selected = Some(next);
                    let top = self
                        .top
                        .clamp(next.saturating_add(1).saturating_sub(page), next);
                    self.top_dirty |= top != self.top;
                    self.top = top;
                }
                1 | 2 | 8 | 9 if self.select_columns && self.selected.is_some() => {
                    let old = self.col.map_or(-1, |column| column as i32);
                    let direction = if action.0 == 2 || action.0 == 9 {
                        -1
                    } else {
                        1
                    };
                    let next = old + direction;
                    if !(0..self.columns.len() as i32).contains(&next) {
                        return Ok(());
                    }
                    self.col = Some(next as usize);
                }
                _ => return Ok(()),
            }
        }
        self.selection_dirty = true;
        if self.seen {
            event(
                events,
                "TableSelectChange",
                name,
                self.selected.map_or(0, |n| n as i32 + 1),
                Some(self.col.map_or(0, |n| n as i32 + 1)),
            )?;
        }
        if let Some(col) = click_col {
            event(
                events,
                "OnTableClick",
                name,
                self.selected.map_or(-1, |n| n as i32),
                Some(col),
            )?;
        }
        Ok(())
    }

    pub(super) fn draw(&mut self, rect: [i32; 4], canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        self.seen = true;
        if let Some(image) = &self.back {
            image.draw(rect, canvas, out);
        }
        let x = cuts(rect[0], &self.columns);
        let y = cuts(rect[1], &self.heights);
        let header = usize::from(self.header.is_some());
        let rows = self
            .header
            .iter()
            .chain(self.rows.iter().skip(self.top))
            .take(self.heights.len());
        for (index, row) in rows.clone().enumerate() {
            if row.color[3] > 0 {
                paint(
                    &self.mask,
                    canvas,
                    [rect[0], y[index], rect[2], y[index + 1]],
                    FULL_UV,
                    row.color,
                    PictureColor::AlphaMask,
                    out,
                );
            }
        }
        if let Some(image) = &self.highlight
            && let Some(row) = self.selected.and_then(|row| row.checked_sub(self.top))
            && row + header < self.heights.len()
        {
            let row = row + header;
            let [left, right] = self
                .col
                .filter(|_| self.select_columns)
                .map_or([rect[0], rect[2]], |col| [x[col], x[col + 1]]);
            image.draw([left, y[row], right, y[row + 1]], canvas, out);
        }
        self.grid.draw(rect, canvas, &x, &y, out);
        for (r, row) in rows.enumerate() {
            let separator = if header == 1 && r == 0 {
                self.grid.header
            } else {
                self.grid.horizontal
            };
            for (c, cell) in row.cells.iter().enumerate() {
                cell.draw(
                    [x[c], y[r]],
                    canvas,
                    self.heights[r],
                    separator,
                    self.grid.padding[1],
                    out,
                );
            }
        }
    }
}

impl FontStyle {
    fn load(shared: &Shared, setting: &Setting<'_>, kind: &str) -> Result<Self> {
        let value = |suffix| setting(&format!("font{kind}{suffix}"));
        let scale = value("scale").map_or(Ok(1.0_f32), str::parse)?;
        ensure!(
            scale.is_finite() && scale >= 0.0,
            "invalid table font scale"
        );
        Ok(Self {
            font: value("")
                .map(|font| load_font(shared, font))
                .transpose()?
                .flatten(),
            color: ini_color(value("color"), [255; 4])?,
            scale,
            horizontal: horizontal(value("alignment"), Alignment::Left),
            vertical: vertical(value("valignment"), Vertical::Top),
        })
    }
}

impl Cell {
    fn draw(
        &self,
        origin: [i32; 2],
        canvas: [i32; 2],
        height: i32,
        separator: i32,
        padding: i32,
        out: &mut Vec<InterfaceDraw>,
    ) {
        let Some(font) = &self.font else {
            return;
        };
        let gap = height
            .saturating_sub(padding.saturating_mul(2))
            .saturating_sub(separator) as f32
            - self.line_height * self.lines.len() as f32;
        let vertical = match self.vertical {
            Vertical::Top => 0.0,
            Vertical::Center => gap * 0.5,
            Vertical::Bottom => gap,
        };
        let [x, y] = [
            origin[0] as f32 + self.offset[0],
            origin[1] as f32 + self.offset[1] + vertical,
        ];
        let mut line_y = 0.0;
        for (text, dx, dy) in &self.lines {
            line_y = dy.unwrap_or(line_y);
            let next = line_y + self.advance;
            if next >= height as f32 {
                break;
            }
            if !text.is_empty() {
                out.push(InterfaceDraw::Text(TextDraw {
                    font: font.clone(),
                    text: text.clone(),
                    canvas,
                    position: [(x + *dx as f32) as i32, (y + line_y) as i32],
                    vertical_anchor: None,
                    scale: self.scale,
                    alignment: Alignment::Left,
                    shadow: true,
                    rgba: self.color,
                }));
            }
            line_y = next;
        }
    }
}

impl Grid {
    fn load(nodes: &mut Nodes, shared: &Shared, setting: &Setting<'_>) -> Result<Self> {
        let atlas = match setting("bordericongroup") {
            Some(group) => nodes.atlas(shared, group)?,
            None => super::Atlas::default(),
        };
        let uv = [
            "borderlefttop",
            "borderrighttop",
            "borderleftbottom",
            "borderrightbottom",
            "bordertop",
            "borderbottom",
            "borderleft",
            "borderright",
            "borderhline",
            "bordervline",
        ]
        .map(|key| atlas.named_part(setting(key)).map(|part| part.0));
        let integer = |key| setting(key).map_or(Ok(0), ini_integer);
        Ok(Self {
            texture: atlas.texture,
            uv,
            rgba: ini_color(setting("bordercolor"), NEUTRAL)?,
            border: integer("borderwidth")?,
            vertical: integer("vlinewidth")?,
            horizontal: integer("hlineheight")?,
            header: integer("headerlineheight")?,
            segmented: flag(setting("hlineisbreakable"), true),
            corner: setting("bordercornersize").map_or(Ok([9, 9]), tuple)?,
            padding: setting("cellspacesize").map_or(Ok([4, 4]), tuple)?,
        })
    }

    fn draw(
        &self,
        [left, top, right, bottom]: [i32; 4],
        canvas: [i32; 2],
        xs: &[i32],
        ys: &[i32],
        out: &mut Vec<InterfaceDraw>,
    ) {
        let Some(texture) = &self.texture else {
            return;
        };
        let mut tile = |index: usize, rect: [i32; 4]| {
            if let Some(uv) = self.uv[index]
                && rect[0] < rect[2]
                && rect[1] < rect[3]
            {
                paint(
                    texture,
                    canvas,
                    rect,
                    uv,
                    self.rgba,
                    PictureColor::Modulate,
                    out,
                );
            }
        };
        let [cw, ch] = self.corner;
        let b = self.border;
        for (row, &y) in ys[1..ys.len() - 1].iter().enumerate() {
            let thickness = if row == 0 {
                self.header
            } else {
                self.horizontal
            };
            if self.segmented {
                for range in xs.windows(2) {
                    tile(8, [range[0], y, range[1], y.saturating_add(thickness)]);
                }
            } else {
                tile(
                    8,
                    [
                        left.saturating_add(b),
                        y,
                        right.saturating_sub(b),
                        y.saturating_add(thickness),
                    ],
                );
            }
        }
        for &x in &xs[1..xs.len() - 1] {
            tile(
                9,
                [
                    x,
                    top.saturating_add(b),
                    x.saturating_add(self.vertical),
                    bottom.saturating_sub(b),
                ],
            );
        }
        let [lx, ty, rx, by] = [
            left.saturating_add(cw),
            top.saturating_add(ch),
            right.saturating_sub(cw),
            bottom.saturating_sub(ch),
        ];
        for (i, rect) in [
            [left, top, lx, ty],
            [rx, top, right, ty],
            [left, by, lx, bottom],
            [rx, by, right, bottom],
            [lx, top, rx, top.saturating_add(b)],
            [lx, bottom.saturating_sub(b), rx, bottom],
            [left, ty, left.saturating_add(b), by],
            [right.saturating_sub(b), ty, right, by],
        ]
        .into_iter()
        .enumerate()
        {
            tile(i, rect);
        }
    }
}

fn horizontal(value: Option<&str>, default: Alignment) -> Alignment {
    match value.map(str::to_ascii_lowercase).as_deref() {
        Some("left") => Alignment::Left,
        Some("center") => Alignment::Center,
        Some("right") => Alignment::Right,
        _ => default,
    }
}

fn vertical(value: Option<&str>, default: Vertical) -> Vertical {
    match value.map(str::to_ascii_lowercase).as_deref() {
        Some("top") => Vertical::Top,
        Some("center") => Vertical::Center,
        Some("bottom") => Vertical::Bottom,
        _ => default,
    }
}

fn dimensions(values: Option<&str>, count: Option<&str>, available: i32) -> Result<Vec<i32>> {
    let count = count.map_or(Ok(1), ini_integer)? as usize;
    ensure!(
        (1..=MAX_AXES).contains(&count),
        "interface table dimension budget"
    );
    let mut sizes = Vec::with_capacity(count);
    for value in values
        .unwrap_or_default()
        .split(',')
        .filter(|value| !value.trim_ascii().is_empty())
    {
        ensure!(sizes.len() < MAX_AXES, "interface table dimension budget");
        sizes.push(ini_integer(value)?.max(0));
    }
    sizes.resize(count, 0);
    let unspecified = sizes.iter().filter(|&&value| value == 0).count();
    if unspecified > 0 {
        let remainder = available.saturating_sub(edge(0, &sizes)).max(0) / unspecified as i32;
        for size in sizes.iter_mut().filter(|size| **size == 0) {
            *size = remainder;
        }
    }
    Ok(sizes)
}

fn edge(origin: i32, sizes: &[i32]) -> i32 {
    sizes
        .iter()
        .fold(origin, |edge, &size| edge.saturating_add(size))
}

fn cuts(origin: i32, sizes: &[i32]) -> Vec<i32> {
    std::iter::once(origin)
        .chain(sizes.iter().scan(origin, |cut, &size| {
            *cut = cut.saturating_add(size);
            Some(*cut)
        }))
        .collect()
}

fn paint(
    texture: &Rc<Texture>,
    canvas: [i32; 2],
    rect: [i32; 4],
    uv: [f32; 4],
    rgba: [u8; 4],
    color: PictureColor,
    out: &mut Vec<InterfaceDraw>,
) {
    out.push(InterfaceDraw::Picture(PictureDraw {
        texture: texture.clone(),
        canvas,
        rect: rect.map(|n| n as f32),
        uv,
        rgba,
        color,
    }));
}

fn event(
    events: &mut NodeEvents,
    name: &str,
    node: &str,
    row: i32,
    col: Option<i32>,
) -> Result<()> {
    let mut values = vec![Value::String(node.to_owned()), Value::Long(row)];
    let format = if let Some(col) = col {
        values.push(Value::Long(col));
        "sll"
    } else {
        "sl"
    };
    events.push((name.to_owned(), Message::new(format, values)?));
    Ok(())
}
