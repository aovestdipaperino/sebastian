//! xychart-beta support: parser subset, `xychartDb.ts` semantics, and a
//! direct port of the chartBuilder (orchestrator, band/linear axes, bar and
//! line plots) plus `xychartRenderer.ts`.

#![allow(
    clippy::assigning_clones,
    clippy::struct_excessive_bools,
    clippy::neg_cmp_op_on_partial_ord
)]

use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};

/// A parse error for xychart source.
#[derive(Debug)]
pub struct XyParseError {
    pub message: String,
}

impl std::fmt::Display for XyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "xychart parse error: {}", self.message)
    }
}

impl std::error::Error for XyParseError {}

// ---------------------------------------------------------------- config --

const CHART_WIDTH: f64 = 700.0;
const CHART_HEIGHT: f64 = 500.0;
const TITLE_FONT_SIZE: f64 = 20.0;
const TITLE_PADDING: f64 = 10.0;
const PLOT_RESERVED_PCT: f64 = 50.0;
const LABEL_FONT_SIZE: f64 = 14.0;
const LABEL_PADDING: f64 = 5.0;
const AXIS_TITLE_FONT_SIZE: f64 = 16.0;
const AXIS_TITLE_PADDING: f64 = 5.0;
const TICK_LENGTH: f64 = 5.0;
const TICK_WIDTH: f64 = 2.0;
const AXIS_LINE_WIDTH: f64 = 2.0;
const BAR_WIDTH_TO_TICK_WIDTH_RATIO: f64 = 0.7;
const MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL: f64 = 0.2;

const PLOT_COLOR_PALETTE: &[&str] = &[
    "#ECECFF", "#8493A6", "#FFC3A0", "#DCDDE1", "#B8E994", "#D1A36F", "#C3CDE6", "#FFB6C1",
    "#496078", "#F8F3E3",
];

// -------------------------------------------------------------- measuring --

/// `computeDimensionOfText` on a tspan inside the styled svg: Trebuchet
/// advance width and the integer line-box height.
struct XyMeasurer {
    m: crate::text::TextMeasurer,
}

impl XyMeasurer {
    fn new() -> Self {
        Self {
            m: crate::text::TextMeasurer::new(),
        }
    }
    fn dim(&self, text: &str, font_size: f64) -> (f64, f64) {
        if text.is_empty() {
            return (0.0, 0.0);
        }
        let width = self.m.measure_width(text, font_size);
        let ascent = (1923.0 * font_size / 2048.0).round();
        let descent = (455.0 * font_size / 2048.0).round();
        let mut height = ascent + descent;
        if (font_size - 16.0).abs() < f64::EPSILON {
            // Chrome reports the 16px tspan line box one f32 ulp above 19
            // inside the sized chart svg (empirical).
            height = f64::from(f32::from_bits(19.0f32.to_bits() + 1));
        }
        (width, height)
    }
    fn max_dimension(&self, texts: &[String], font_size: f64) -> (f64, f64) {
        let mut w = 0.0f64;
        let mut h = 0.0f64;
        for t in texts {
            let (tw, th) = self.dim(t, font_size);
            w = w.max(tw);
            h = h.max(th);
        }
        (w, h)
    }
}

// ------------------------------------------------------------------- data --

#[derive(Debug, Clone)]
enum XAxisData {
    Band(Vec<String>),
    Linear { min: f64, max: f64 },
}

#[derive(Debug, Clone)]
enum PlotKind {
    Line,
    Bar,
}

#[derive(Debug, Clone)]
struct Plot {
    kind: PlotKind,
    color: String,
    /// (category, value) pairs; category is a string even for linear axes.
    data: Vec<(String, f64)>,
}

#[derive(Debug)]
struct Db {
    title: String,
    orientation_horizontal: bool,
    x_title: String,
    x_axis: XAxisData,
    has_set_x: bool,
    y_title: String,
    y_min: f64,
    y_max: f64,
    has_set_y: bool,
    plots: Vec<Plot>,
    plot_index: usize,
}

impl Default for Db {
    fn default() -> Self {
        Self {
            title: String::new(),
            orientation_horizontal: false,
            x_title: String::new(),
            x_axis: XAxisData::Band(Vec::new()),
            has_set_x: false,
            y_title: String::new(),
            y_min: f64::INFINITY,
            y_max: f64::NEG_INFINITY,
            has_set_y: false,
            plots: Vec::new(),
            plot_index: 0,
        }
    }
}

impl Db {
    fn transform_data(&mut self, data: &[f64]) -> Vec<(String, f64)> {
        if data.is_empty() {
            return Vec::new();
        }
        if !self.has_set_x {
            let (prev_min, prev_max) = match &self.x_axis {
                XAxisData::Linear { min, max } => (*min, *max),
                XAxisData::Band(_) => (f64::INFINITY, f64::NEG_INFINITY),
            };
            #[allow(clippy::cast_precision_loss)]
            let n = data.len() as f64;
            self.x_axis = XAxisData::Linear {
                min: prev_min.min(1.0),
                max: prev_max.max(n),
            };
            self.has_set_x = true;
        }
        if !self.has_set_y {
            let min_v = data.iter().copied().fold(f64::INFINITY, f64::min);
            let max_v = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            self.y_min = self.y_min.min(min_v);
            self.y_max = self.y_max.max(max_v);
        }
        match &self.x_axis {
            XAxisData::Band(categories) => categories
                .iter()
                .zip(data)
                .map(|(c, v)| (c.clone(), *v))
                .collect(),
            XAxisData::Linear { min, max } => {
                #[allow(clippy::cast_precision_loss)]
                let step = (max - min) / (data.len() as f64 - 1.0);
                let mut categories: Vec<String> = Vec::new();
                let mut i = *min;
                while i <= *max {
                    categories.push(js_num(i));
                    i += step;
                }
                categories
                    .iter()
                    .zip(data)
                    .map(|(c, v)| (c.clone(), *v))
                    .collect()
            }
        }
    }

    fn plot_color(&self) -> String {
        let idx = if self.plot_index == 0 {
            0
        } else {
            self.plot_index % PLOT_COLOR_PALETTE.len()
        };
        PLOT_COLOR_PALETTE[idx].to_owned()
    }
}

// ------------------------------------------------------------------ parse --

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_owned()
}

/// `"title" rest` → (title, rest); or single unquoted token.
fn take_text(s: &str) -> (String, &str) {
    let s = s.trim();
    if let Some(r) = s.strip_prefix('"')
        && let Some(end) = r.find('"')
    {
        return (r[..end].to_owned(), r[end + 1..].trim());
    }
    // Unquoted: up to a '[' or a number range.
    (String::new(), s)
}

fn parse_range(s: &str) -> Option<(f64, f64)> {
    let (a, b) = s.split_once("-->")?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn parse_number_list(s: &str) -> Option<Vec<f64>> {
    let inner = s.trim().strip_prefix('[')?.strip_suffix(']')?;
    inner
        .split(',')
        .map(|t| t.trim().parse::<f64>().ok())
        .collect()
}

fn parse_category_list(s: &str) -> Option<Vec<String>> {
    let inner = s.trim().strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.split(',').map(unquote).collect())
}

fn parse(source: &str) -> Result<Db, XyParseError> {
    let mut db = Db::default();
    let mut found_header = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            let Some(rest) = line.strip_prefix("xychart-beta") else {
                return Err(XyParseError {
                    message: format!("expected xychart-beta header, got {line:?}"),
                });
            };
            found_header = true;
            if rest.trim() == "horizontal" {
                db.orientation_horizontal = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("title") {
            db.title = unquote(rest);
            continue;
        }
        if let Some(rest) = line.strip_prefix("x-axis") {
            let (title, rest2) = take_text(rest);
            let has_title = !title.is_empty();
            if has_title {
                db.x_title = title;
            }
            if let Some(categories) = parse_category_list(rest2) {
                db.x_axis = XAxisData::Band(categories);
                db.has_set_x = true;
            } else if let Some((min, max)) = parse_range(rest2) {
                db.x_axis = XAxisData::Linear { min, max };
                db.has_set_x = true;
            } else if !rest2.is_empty() && !has_title {
                db.x_title = rest2.to_owned();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("y-axis") {
            let (title, rest2) = take_text(rest);
            let has_title = !title.is_empty();
            if has_title {
                db.y_title = title;
            }
            if let Some((min, max)) = parse_range(rest2) {
                db.y_min = min;
                db.y_max = max;
                db.has_set_y = true;
            } else if !rest2.is_empty() && !has_title {
                db.y_title = rest2.to_owned();
            }
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("bar")
            .or_else(|| line.strip_prefix("line"))
        {
            let kind = if line.starts_with("bar") {
                PlotKind::Bar
            } else {
                PlotKind::Line
            };
            let (_title, rest2) = take_text(rest);
            let Some(values) = parse_number_list(rest2) else {
                return Err(XyParseError {
                    message: format!("bad plot data: {line}"),
                });
            };
            let data = db.transform_data(&values);
            let color = db.plot_color();
            db.plots.push(Plot { kind, color, data });
            db.plot_index += 1;
            continue;
        }
        return Err(XyParseError {
            message: format!("unsupported xychart statement: {line}"),
        });
    }
    if !found_header {
        return Err(XyParseError {
            message: "missing xychart-beta header".to_owned(),
        });
    }
    if db.plots.is_empty() {
        return Err(XyParseError {
            message: "No Plot to render".to_owned(),
        });
    }
    Ok(db)
}

// ------------------------------------------------------------------- d3 ----

/// d3-array `ticks(start, stop, count)`.
fn d3_ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    fn tick_spec(start: f64, stop: f64, count: f64) -> (f64, f64, f64) {
        let e10 = 50.0f64.sqrt();
        let e5 = 10.0f64.sqrt();
        let e2 = 2.0f64.sqrt();
        let step = (stop - start) / count.max(0.0);
        let power = step.log10().floor();
        let error = step / 10.0f64.powf(power);
        let factor = if error >= e10 {
            10.0
        } else if error >= e5 {
            5.0
        } else if error >= e2 {
            2.0
        } else {
            1.0
        };
        let (i1, i2, inc);
        if power < 0.0 {
            let mut inc_ = 10.0f64.powf(-power) / factor;
            let mut i1_ = (start * inc_).round();
            let mut i2_ = (stop * inc_).round();
            if i1_ / inc_ < start {
                i1_ += 1.0;
            }
            if i2_ / inc_ > stop {
                i2_ -= 1.0;
            }
            inc_ = -inc_;
            i1 = i1_;
            i2 = i2_;
            inc = inc_;
        } else {
            let inc_ = 10.0f64.powf(power) * factor;
            let mut i1_ = (start / inc_).round();
            let mut i2_ = (stop / inc_).round();
            if i1_ * inc_ < start {
                i1_ += 1.0;
            }
            if i2_ * inc_ > stop {
                i2_ -= 1.0;
            }
            i1 = i1_;
            i2 = i2_;
            inc = inc_;
        }
        if i2 < i1 && (0.5..2.0).contains(&count) {
            return tick_spec(start, stop, count * 2.0);
        }
        (i1, i2, inc)
    }

    if !(count > 0.0) {
        return Vec::new();
    }
    if start == stop {
        return vec![start];
    }
    let reverse = stop < start;
    let (i1, i2, inc) = if reverse {
        tick_spec(stop, start, count)
    } else {
        tick_spec(start, stop, count)
    };
    if i2 < i1 {
        return Vec::new();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (i2 - i1 + 1.0) as usize;
    let mut ticks = Vec::with_capacity(n);
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let fi = i as f64;
        let v = if reverse {
            if inc < 0.0 {
                (i2 - fi) / -inc
            } else {
                (i2 - fi) * inc
            }
        } else if inc < 0.0 {
            (i1 + fi) / -inc
        } else {
            (i1 + fi) * inc
        };
        ticks.push(v);
    }
    ticks
}

// ------------------------------------------------------------------- axes --

enum Scale {
    Linear {
        domain: (f64, f64),
        range: (f64, f64),
    },
    Band {
        categories: Vec<String>,
        range: (f64, f64),
    },
}

impl Scale {
    fn value(&self, tick: &str) -> f64 {
        match self {
            Scale::Linear { domain, range } => {
                let v: f64 = tick.parse().unwrap_or(0.0);
                // d3 bimap: a descending domain swaps both ends.
                let ((d0, d1), (r0, r1)) = if domain.1 < domain.0 {
                    ((domain.1, domain.0), (range.1, range.0))
                } else {
                    (*domain, *range)
                };
                let t = (v - d0) / (d1 - d0);
                // d3 interpolateNumber: a * (1 - t) + b * t.
                r0 * (1.0 - t) + r1 * t
            }
            Scale::Band { categories, range } => {
                // paddingInner 1, paddingOuter 0, align 0.5, round false.
                #[allow(clippy::cast_precision_loss)]
                let n = categories.len() as f64;
                let step = (range.1 - range.0) / 1.0f64.max(n - 1.0);
                let start = range.0 + (range.1 - range.0 - step * (n - 1.0)) * 0.5;
                categories
                    .iter()
                    .position(|c| c == tick)
                    .map_or(range.0, |i| {
                        #[allow(clippy::cast_precision_loss)]
                        let fi = i as f64;
                        start + step * fi
                    })
            }
        }
    }
}

struct Axis {
    // config
    title: String,
    is_band: bool,
    band_categories: Vec<String>,
    linear_domain: (f64, f64),
    // state
    position: &'static str, // "left" | "bottom" | "top"
    range: (f64, f64),
    outer_padding: f64,
    show_title: bool,
    show_label: bool,
    show_tick: bool,
    show_axis_line: bool,
    title_text_height: f64,
    bounding: (f64, f64, f64, f64), // x, y, w, h
}

impl Axis {
    fn new_band(categories: Vec<String>, title: String) -> Self {
        Self {
            title,
            is_band: true,
            band_categories: categories,
            linear_domain: (0.0, 0.0),
            position: "left",
            range: (0.0, 10.0),
            outer_padding: 0.0,
            show_title: false,
            show_label: false,
            show_tick: false,
            show_axis_line: false,
            title_text_height: 0.0,
            bounding: (0.0, 0.0, 0.0, 0.0),
        }
    }
    fn new_linear(domain: (f64, f64), title: String) -> Self {
        Self {
            is_band: false,
            band_categories: Vec::new(),
            linear_domain: domain,
            ..Self::new_band(Vec::new(), title)
        }
    }

    fn get_range(&self) -> (f64, f64) {
        (
            self.range.0 + self.outer_padding,
            self.range.1 - self.outer_padding,
        )
    }

    fn scale(&self) -> Scale {
        if self.is_band {
            Scale::Band {
                categories: self.band_categories.clone(),
                range: self.get_range(),
            }
        } else {
            let mut domain = self.linear_domain;
            if self.position == "left" {
                domain = (domain.1, domain.0);
            }
            Scale::Linear {
                domain,
                range: self.get_range(),
            }
        }
    }

    fn tick_values(&self) -> Vec<String> {
        if self.is_band {
            self.band_categories.clone()
        } else {
            // scale.ticks() with count 10 on the current (possibly reversed)
            // domain.
            let mut domain = self.linear_domain;
            if self.position == "left" {
                domain = (domain.1, domain.0);
            }
            d3_ticks(domain.0, domain.1, 10.0)
                .into_iter()
                .map(js_num)
                .collect()
        }
    }

    fn scale_value(&self, tick: &str) -> f64 {
        self.scale().value(tick)
    }

    fn tick_distance(&self) -> f64 {
        let range = self.get_range();
        #[allow(clippy::cast_precision_loss)]
        let n = self.tick_values().len() as f64;
        (range.0 - range.1).abs() / n
    }

    fn set_range(&mut self, range: (f64, f64)) {
        self.range = range;
        if self.position == "left" {
            self.bounding.3 = range.1 - range.0;
        } else {
            self.bounding.2 = range.1 - range.0;
        }
    }

    fn recalculate_outer_padding_to_draw_bar(&mut self) {
        if BAR_WIDTH_TO_TICK_WIDTH_RATIO * self.tick_distance() > self.outer_padding * 2.0 {
            self.outer_padding =
                ((BAR_WIDTH_TO_TICK_WIDTH_RATIO * self.tick_distance()) / 2.0).floor();
        }
    }

    fn label_dimension(&self, measurer: &XyMeasurer) -> (f64, f64) {
        measurer.max_dimension(&self.tick_values(), LABEL_FONT_SIZE)
    }

    fn calculate_space(&mut self, measurer: &XyMeasurer, avail: (f64, f64)) -> (f64, f64) {
        if self.position == "left" {
            let mut available_width = avail.0;
            if available_width > AXIS_LINE_WIDTH {
                available_width -= AXIS_LINE_WIDTH;
                self.show_axis_line = true;
            }
            {
                let space = self.label_dimension(measurer);
                let max_padding = MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL * avail.1;
                self.outer_padding = (space.1 / 2.0).min(max_padding);
                let width_required = space.0 + LABEL_PADDING * 2.0;
                if width_required <= available_width {
                    available_width -= width_required;
                    self.show_label = true;
                }
            }
            if available_width >= TICK_LENGTH {
                self.show_tick = true;
                available_width -= TICK_LENGTH;
            }
            if !self.title.is_empty() {
                let space =
                    measurer.max_dimension(std::slice::from_ref(&self.title), AXIS_TITLE_FONT_SIZE);
                let width_required = space.1 + AXIS_TITLE_PADDING * 2.0;
                self.title_text_height = space.1;
                if width_required <= available_width {
                    available_width -= width_required;
                    self.show_title = true;
                }
            }
            self.bounding.2 = avail.0 - available_width;
            self.bounding.3 = avail.1;
        } else {
            let mut available_height = avail.1;
            if available_height > AXIS_LINE_WIDTH {
                available_height -= AXIS_LINE_WIDTH;
                self.show_axis_line = true;
            }
            {
                let space = self.label_dimension(measurer);
                let max_padding = MAX_OUTER_PADDING_PERCENT_FOR_WRT_LABEL * avail.0;
                self.outer_padding = (space.0 / 2.0).min(max_padding);
                let height_required = space.1 + LABEL_PADDING * 2.0;
                if height_required <= available_height {
                    available_height -= height_required;
                    self.show_label = true;
                }
            }
            if available_height >= TICK_LENGTH {
                self.show_tick = true;
                available_height -= TICK_LENGTH;
            }
            if !self.title.is_empty() {
                let space =
                    measurer.max_dimension(std::slice::from_ref(&self.title), AXIS_TITLE_FONT_SIZE);
                let height_required = space.1 + AXIS_TITLE_PADDING * 2.0;
                self.title_text_height = space.1;
                if height_required <= available_height {
                    available_height -= height_required;
                    self.show_title = true;
                }
            }
            self.bounding.2 = avail.0;
            self.bounding.3 = avail.1 - available_height;
        }
        (self.bounding.2, self.bounding.3)
    }
}

// ---------------------------------------------------------------- drawing --

/// Renders xychart-beta source to a complete SVG document string.
///
/// # Errors
/// Returns a [`XyParseError`] when the source is not a valid xychart.
#[allow(clippy::too_many_lines)]
pub fn render_xychart(source: &str, id: &str) -> Result<String, XyParseError> {
    let config = crate::render::config::detect_init(source);
    let hand_drawn = config.is_hand_drawn();
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let measurer = XyMeasurer::new();

    let text_color = {
        let t = crate::render::themes::get(&theme_vars, "primaryTextColor");
        if t.is_empty() {
            "#131300".to_owned()
        } else {
            t
        }
    };
    let background = {
        let b = crate::render::themes::get(&theme_vars, "background");
        if b.is_empty() { "white".to_owned() } else { b }
    };

    // Axes.
    let mut x_axis = match &db.x_axis {
        XAxisData::Band(c) => Axis::new_band(c.clone(), db.x_title.clone()),
        XAxisData::Linear { min, max } => Axis::new_linear((*min, *max), db.x_title.clone()),
    };
    let mut y_axis = Axis::new_linear((db.y_min, db.y_max), db.y_title.clone());

    // Title space.
    let title_dim = measurer.max_dimension(std::slice::from_ref(&db.title), TITLE_FONT_SIZE);
    let show_title = !db.title.is_empty();
    let title_height = if show_title {
        title_dim.1 + 2.0 * TITLE_PADDING
    } else {
        0.0
    };

    // Orchestrator.calculateVerticalSpace (horizontal orientation is not
    // exercised by the corpus; vertical only).
    let mut available_width = CHART_WIDTH;
    let mut available_height = CHART_HEIGHT;
    let mut chart_width = ((available_width * PLOT_RESERVED_PCT) / 100.0).floor();
    let mut chart_height = ((available_height * PLOT_RESERVED_PCT) / 100.0).floor();
    available_width -= chart_width;
    available_height -= chart_height;
    available_height -= title_height;
    let plot_y_start = title_height;

    x_axis.position = "bottom";
    let used = x_axis.calculate_space(&measurer, (available_width, available_height));
    available_height -= used.1;
    y_axis.position = "left";
    let used = y_axis.calculate_space(&measurer, (available_width, available_height));
    let plot_x = used.0;
    available_width -= used.0;
    if available_width > 0.0 {
        chart_width += available_width;
    }
    if available_height > 0.0 {
        chart_height += available_height;
    }
    let plot_y = plot_y_start;

    x_axis.set_range((plot_x, plot_x + chart_width));
    x_axis.bounding.0 = plot_x;
    x_axis.bounding.1 = plot_y + chart_height;
    y_axis.set_range((plot_y, plot_y + chart_height));
    y_axis.bounding.0 = 0.0;
    y_axis.bounding.1 = plot_y;
    if db.plots.iter().any(|p| matches!(p.kind, PlotKind::Bar)) {
        x_axis.recalculate_outer_padding_to_draw_bar();
    }

    // --- build SVG ---
    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(CHART_WIDTH)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(CHART_WIDTH), js_num(CHART_HEIGHT)),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "xychart");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_xychart_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    let main = append(&svg, "g");
    set_attr(&main, "class", "main");
    let bg = append(&main, "rect");
    set_attr(&bg, "width", js_num(CHART_WIDTH));
    set_attr(&bg, "height", js_num(CHART_HEIGHT));
    set_attr(&bg, "class", "background");
    set_attr(&bg, "fill", &background);

    let text_elem = |parent: &Element,
                     text: &str,
                     x: f64,
                     y: f64,
                     fill: &str,
                     font_size: f64,
                     rotation: f64,
                     vertical: &str,
                     horizontal: &str| {
        let t = append(parent, "text");
        set_attr(&t, "x", "0");
        set_attr(&t, "y", "0");
        set_attr(&t, "fill", fill);
        set_attr(&t, "font-size", js_num(font_size));
        set_attr(
            &t,
            "dominant-baseline",
            if vertical == "top" {
                "text-before-edge"
            } else {
                "middle"
            },
        );
        set_attr(
            &t,
            "text-anchor",
            match horizontal {
                "left" => "start",
                "right" => "end",
                _ => "middle",
            },
        );
        set_attr(
            &t,
            "transform",
            format!(
                "translate({}, {}) rotate({})",
                js_num(x),
                js_num(y),
                js_num(rotation)
            ),
        );
        set_text(&t, text);
    };
    let path_elem = |parent: &Element, d: &str, fill: Option<&str>, stroke: &str, sw: f64| {
        let p = append(parent, "path");
        set_attr(&p, "d", d);
        set_attr(&p, "fill", fill.unwrap_or("none"));
        set_attr(&p, "stroke", stroke);
        set_attr(&p, "stroke-width", js_num(sw));
    };

    // Title group (drawn first: componentStore order is title, plot, axes).
    if show_title {
        let g = append(&main, "g");
        set_attr(&g, "class", "chart-title");
        text_elem(
            &g,
            &db.title,
            CHART_WIDTH / 2.0,
            title_height / 2.0,
            &text_color,
            TITLE_FONT_SIZE,
            0.0,
            "middle",
            "center",
        );
    }

    // Plot group.
    let plot_g = append(&main, "g");
    set_attr(&plot_g, "class", "plot");
    for (i, plot) in db.plots.iter().enumerate() {
        match plot.kind {
            PlotKind::Bar => {
                let g = append(&plot_g, "g");
                set_attr(&g, "class", format!("bar-plot-{i}"));
                let bar_padding_percent = 0.05;
                let bar_width = x_axis
                    .outer_padding
                    .mul_add(2.0, 0.0)
                    .min(x_axis.tick_distance())
                    * (1.0 - bar_padding_percent);
                let half = bar_width / 2.0;
                for (cat, val) in &plot.data {
                    let x = x_axis.scale_value(cat);
                    let y = y_axis.scale_value(&js_num(*val));
                    let rect = append(&g, "rect");
                    set_attr(&rect, "x", js_num(x - half));
                    set_attr(&rect, "y", js_num(y));
                    set_attr(&rect, "width", js_num(bar_width));
                    set_attr(&rect, "height", js_num(plot_y + chart_height - y));
                    set_attr(&rect, "fill", plot.color.clone());
                    set_attr(&rect, "stroke", plot.color.clone());
                    set_attr(&rect, "stroke-width", "0");
                    if hand_drawn {
                        crate::render::handdrawn::hd_overlay_rect(
                            &g,
                            x - half,
                            y,
                            bar_width,
                            plot_y + chart_height - y,
                            &plot.color,
                            "",
                        );
                    }
                }
            }
            PlotKind::Line => {
                let g = append(&plot_g, "g");
                set_attr(&g, "class", format!("line-plot-{i}"));
                // d3.line() with the digits(3) serializer.
                let round3 = |v: f64| js_num(((v * 1000.0) + 0.5).floor() / 1000.0);
                let mut d = String::new();
                for (idx, (cat, val)) in plot.data.iter().enumerate() {
                    let x = x_axis.scale_value(cat);
                    let y = y_axis.scale_value(&js_num(*val));
                    if idx == 0 {
                        d.push('M');
                    } else {
                        d.push('L');
                    }
                    d.push_str(&round3(x));
                    d.push(',');
                    d.push_str(&round3(y));
                }
                path_elem(&g, &d, None, &plot.color, 2.0);
            }
        }
    }

    // Bottom (x) axis.
    {
        let g = append(&main, "g");
        set_attr(&g, "class", "bottom-axis");
        let br = x_axis.bounding;
        if x_axis.show_axis_line {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "axis-line");
            let y = br.1 + AXIS_LINE_WIDTH / 2.0;
            path_elem(
                &gg,
                &format!(
                    "M {},{} L {},{}",
                    js_num(br.0),
                    js_num(y),
                    js_num(br.0 + br.2),
                    js_num(y)
                ),
                None,
                &text_color,
                AXIS_LINE_WIDTH,
            );
        }
        if x_axis.show_label {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "label");
            let y = br.1
                + LABEL_PADDING
                + if x_axis.show_tick { TICK_LENGTH } else { 0.0 }
                + if x_axis.show_axis_line {
                    AXIS_LINE_WIDTH
                } else {
                    0.0
                };
            for tick in x_axis.tick_values() {
                text_elem(
                    &gg,
                    &tick,
                    x_axis.scale_value(&tick),
                    y,
                    &text_color,
                    LABEL_FONT_SIZE,
                    0.0,
                    "top",
                    "center",
                );
            }
        }
        if x_axis.show_tick {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "ticks");
            let y = br.1
                + if x_axis.show_axis_line {
                    AXIS_LINE_WIDTH
                } else {
                    0.0
                };
            for tick in x_axis.tick_values() {
                let x = x_axis.scale_value(&tick);
                path_elem(
                    &gg,
                    &format!(
                        "M {},{} L {},{}",
                        js_num(x),
                        js_num(y),
                        js_num(x),
                        js_num(y + TICK_LENGTH)
                    ),
                    None,
                    &text_color,
                    TICK_WIDTH,
                );
            }
        }
        if x_axis.show_title {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "title");
            let range = x_axis.range;
            text_elem(
                &gg,
                &x_axis.title,
                range.0 + (range.1 - range.0) / 2.0,
                br.1 + br.3 - AXIS_TITLE_PADDING - x_axis.title_text_height,
                &text_color,
                AXIS_TITLE_FONT_SIZE,
                0.0,
                "top",
                "center",
            );
        }
    }

    // Left (y) axis.
    {
        let g = append(&main, "g");
        set_attr(&g, "class", "left-axis");
        let br = y_axis.bounding;
        if y_axis.show_axis_line {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "axisl-line");
            let x = br.0 + br.2 - AXIS_LINE_WIDTH / 2.0;
            path_elem(
                &gg,
                &format!(
                    "M {},{} L {},{} ",
                    js_num(x),
                    js_num(br.1),
                    js_num(x),
                    js_num(br.1 + br.3)
                ),
                None,
                &text_color,
                AXIS_LINE_WIDTH,
            );
        }
        if y_axis.show_label {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "label");
            let x = br.0 + br.2
                - LABEL_PADDING
                - if y_axis.show_tick { TICK_LENGTH } else { 0.0 }
                - if y_axis.show_axis_line {
                    AXIS_LINE_WIDTH
                } else {
                    0.0
                };
            for tick in y_axis.tick_values() {
                text_elem(
                    &gg,
                    &tick,
                    x,
                    y_axis.scale_value(&tick),
                    &text_color,
                    LABEL_FONT_SIZE,
                    0.0,
                    "middle",
                    "right",
                );
            }
        }
        if y_axis.show_tick {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "ticks");
            let x = br.0 + br.2
                - if y_axis.show_axis_line {
                    AXIS_LINE_WIDTH
                } else {
                    0.0
                };
            for tick in y_axis.tick_values() {
                let y = y_axis.scale_value(&tick);
                path_elem(
                    &gg,
                    &format!(
                        "M {},{} L {},{}",
                        js_num(x),
                        js_num(y),
                        js_num(x - TICK_LENGTH),
                        js_num(y)
                    ),
                    None,
                    &text_color,
                    TICK_WIDTH,
                );
            }
        }
        if y_axis.show_title {
            let gg = append(&g, "g");
            set_attr(&gg, "class", "title");
            text_elem(
                &gg,
                &y_axis.title,
                br.0 + AXIS_TITLE_PADDING,
                br.1 + br.3 / 2.0,
                &text_color,
                AXIS_TITLE_FONT_SIZE,
                270.0,
                "top",
                "center",
            );
        }
    }

    // db.setTmpSVGG leaves an empty measurement group behind.
    let tmp = append(&svg, "g");
    set_attr(&tmp, "class", "mermaid-tmp-group");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
