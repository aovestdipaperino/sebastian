//! gantt support: parser subset (`gantt.jison` + `ganttDb.js` semantics with
//! naive-local dayjs date arithmetic), and a direct port of
//! `ganttRenderer.js` (d3 scaleTime rangeRound, d3-time ticks, d3-axis
//! bottom markup).

use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};

/// A parse error for gantt source.
#[derive(Debug)]
pub struct GanttParseError {
    pub message: String,
}

impl std::fmt::Display for GanttParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gantt parse error: {}", self.message)
    }
}

impl std::error::Error for GanttParseError {}

// -------------------------------------------------------------- date math --

const MS_PER_DAY: f64 = 86_400_000.0;

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date from days since epoch.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Naive-local timestamp helpers (no DST — matches mmdc for diagrams that
/// do not straddle a transition).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Ts(pub f64);

impl Ts {
    fn from_ymd(y: i64, m: i64, d: i64) -> Self {
        #[allow(clippy::cast_precision_loss)]
        Ts(days_from_civil(y, m, d) as f64 * MS_PER_DAY)
    }
    fn day_number(self) -> i64 {
        #[allow(clippy::cast_possible_truncation)]
        let d = (self.0 / MS_PER_DAY).floor() as i64;
        d
    }
    fn add_months(self, months: i64) -> Self {
        let day = self.day_number();
        let ms_in_day = self.0 - {
            #[allow(clippy::cast_precision_loss)]
            let v = day as f64 * MS_PER_DAY;
            v
        };
        let (y, m, d) = civil_from_days(day);
        let total = y * 12 + (m - 1) + months;
        let ny = total.div_euclid(12);
        let nm = total.rem_euclid(12) + 1;
        // Clamp the day-of-month like dayjs does.
        let dim = days_in_month(ny, nm);
        let nd = d.min(dim);
        #[allow(clippy::cast_precision_loss)]
        Ts(days_from_civil(ny, nm, nd) as f64 * MS_PER_DAY + ms_in_day)
    }
}

fn days_in_month(y: i64, m: i64) -> i64 {
    days_from_civil(
        if m == 12 { y + 1 } else { y },
        if m == 12 { 1 } else { m + 1 },
        1,
    ) - days_from_civil(y, m, 1)
}

/// Strict `dayjs(str, format)` for the common gantt formats.
fn parse_date(s: &str, format: &str) -> Option<Ts> {
    let s = s.trim();
    match format.trim() {
        "YYYY-MM-DD" => {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2
            {
                return None;
            }
            let y: i64 = parts[0].parse().ok()?;
            let m: i64 = parts[1].parse().ok()?;
            let d: i64 = parts[2].parse().ok()?;
            if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
                return None;
            }
            Some(Ts::from_ymd(y, m, d))
        }
        _ => None,
    }
}

/// `dayjs.add(value, unit)` in naive time.
fn add_duration(t: Ts, value: f64, unit: &str) -> Ts {
    match unit {
        "ms" => Ts(t.0 + value),
        "s" => Ts(value.mul_add(1000.0, t.0)),
        "m" => Ts(value.mul_add(60_000.0, t.0)),
        "h" => Ts(value.mul_add(3_600_000.0, t.0)),
        "d" => Ts(value.mul_add(MS_PER_DAY, t.0)),
        "w" => Ts(value.mul_add(7.0 * MS_PER_DAY, t.0)),
        "M" => {
            #[allow(clippy::cast_possible_truncation)]
            let whole = value.trunc() as i64;
            t.add_months(whole)
        }
        "y" => {
            #[allow(clippy::cast_possible_truncation)]
            let whole = value.trunc() as i64;
            t.add_months(whole * 12)
        }
        _ => t,
    }
}

// ------------------------------------------------------------------- data --

#[derive(Debug, Clone)]
struct Task {
    id: String,
    name: String,
    section: String,
    start: Ts,
    end: Ts,
    order: usize,
    active: bool,
    done: bool,
    crit: bool,
    milestone: bool,
}

#[derive(Debug, Default)]
struct Db {
    title: String,
    date_format: String,
    axis_format: String,
    tasks: Vec<Task>,
    task_cnt: usize,
}

impl Db {
    fn find_task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    fn get_start(&self, s: &str) -> Option<Ts> {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix("after ") {
            let mut latest: Option<Ts> = None;
            for id in rest.split(' ') {
                if let Some(t) = self.find_task(id.trim())
                    && latest.is_none_or(|l| t.end > l)
                {
                    latest = Some(t.end);
                }
            }
            return latest;
        }
        parse_date(s, &self.date_format)
    }

    fn get_end(&self, start: Ts, s: &str) -> Ts {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix("until ") {
            for id in rest.split(' ') {
                if let Some(t) = self.find_task(id.trim()) {
                    return t.start;
                }
            }
        }
        if let Some(d) = parse_date(s, &self.date_format) {
            return d;
        }
        // Duration: (\d+(\.\d+)?)([Mdhmswy]|ms)
        let (num_part, unit): (&str, &str) = if let Some(n) = s.strip_suffix("ms") {
            (n, "ms")
        } else if let Some(last) = s.chars().last()
            && "Mdhmswy".contains(last)
        {
            (
                &s[..s.len() - last.len_utf8()],
                &s[s.len() - last.len_utf8()..],
            )
        } else {
            return start;
        };
        num_part
            .trim()
            .parse::<f64>()
            .map_or(start, |v| add_duration(start, v, unit))
    }
}

fn parse(source: &str) -> Result<Db, GanttParseError> {
    let mut db = Db {
        date_format: "YYYY-MM-DD".to_owned(),
        ..Db::default()
    };
    let mut found_header = false;
    let mut section = String::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            if line.starts_with("gantt") {
                found_header = true;
                continue;
            }
            return Err(GanttParseError {
                message: format!("expected gantt header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("title") {
            // The jison lexer consumes exactly one whitespace char after the
            // keyword; the remainder (leading spaces included) is the title.
            db.title = rest.strip_prefix([' ', '\t']).unwrap_or(rest).to_owned();
            continue;
        }
        if let Some(rest) = line.strip_prefix("dateFormat") {
            db.date_format = rest.trim().to_owned();
            continue;
        }
        if let Some(rest) = line.strip_prefix("axisFormat") {
            db.axis_format = rest.trim().to_owned();
            continue;
        }
        if let Some(rest) = line.strip_prefix("section") {
            section = rest.trim().to_owned();
            continue;
        }
        for kw in [
            "excludes",
            "includes",
            "todayMarker",
            "tickInterval",
            "weekday",
            "displayMode",
            "inclusiveEndDates",
            "topAxis",
            "accTitle",
            "accDescr",
        ] {
            if line.starts_with(kw) {
                return Err(GanttParseError {
                    message: format!("unsupported gantt statement: {line}"),
                });
            }
        }
        // Task: name : data
        let Some(colon) = line.find(':') else {
            return Err(GanttParseError {
                message: format!("unsupported gantt statement: {line}"),
            });
        };
        // Task text keeps its raw (untrimmed-right) form up to the colon,
        // minus leading whitespace (the jison lexer captures `[^:\n]+`).
        let name = raw[..raw.find(':').expect("colon present")]
            .trim_start()
            .to_owned();
        let data = line[colon + 1..].trim();
        let mut parts: Vec<String> = data.split(',').map(|p| p.trim().to_owned()).collect();

        let mut active = false;
        let mut done = false;
        let mut crit = false;
        let mut milestone = false;
        loop {
            let Some(first) = parts.first() else { break };
            match first.as_str() {
                "active" => active = true,
                "done" => done = true,
                "crit" => crit = true,
                "milestone" => milestone = true,
                _ => break,
            }
            parts.remove(0);
        }

        let prev_end = db.tasks.last().map(|t| t.end);
        let (id, start, end_data) = match parts.len() {
            1 => {
                db.task_cnt += 1;
                (
                    format!("task{}", db.task_cnt),
                    prev_end.unwrap_or(Ts(0.0)),
                    parts[0].clone(),
                )
            }
            2 => {
                // Either "id, endData" (start from prev/`after`) or
                // "startData, endData".
                if let Some(start) = db.get_start(&parts[0]) {
                    db.task_cnt += 1;
                    (format!("task{}", db.task_cnt), start, parts[1].clone())
                } else {
                    (
                        parts[0].clone(),
                        db.get_start(&parts[1]).or(prev_end).unwrap_or(Ts(0.0)),
                        String::new(),
                    )
                }
            }
            3 => (
                parts[0].clone(),
                db.get_start(&parts[1]).or(prev_end).unwrap_or(Ts(0.0)),
                parts[2].clone(),
            ),
            _ => {
                return Err(GanttParseError {
                    message: format!("bad task data: {line}"),
                });
            }
        };
        let end = db.get_end(start, &end_data);
        let order = db.tasks.len();
        db.tasks.push(Task {
            id,
            name,
            section: section.clone(),
            start,
            end,
            order,
            active,
            done,
            crit,
            milestone,
        });
    }
    if !found_header {
        return Err(GanttParseError {
            message: "missing gantt header".to_owned(),
        });
    }
    if db.tasks.is_empty() {
        return Err(GanttParseError {
            message: "no tasks".to_owned(),
        });
    }
    Ok(db)
}

// ------------------------------------------------------------------- d3 ----

/// Naive-time interval floors.
fn floor_day(t: f64) -> f64 {
    (t / MS_PER_DAY).floor() * MS_PER_DAY
}
fn floor_week(t: f64) -> f64 {
    // timeSunday: epoch day 0 (1970-01-01) was a Thursday (weekday 4).
    let day = (t / MS_PER_DAY).floor();
    let weekday = (day + 4.0).rem_euclid(7.0);
    (day - weekday) * MS_PER_DAY
}
fn floor_month(t: f64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    let (y, m, _) = civil_from_days((t / MS_PER_DAY).floor() as i64);
    #[allow(clippy::cast_precision_loss)]
    let v = days_from_civil(y, m, 1) as f64 * MS_PER_DAY;
    v
}
fn floor_year(t: f64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    let (y, _, _) = civil_from_days((t / MS_PER_DAY).floor() as i64);
    #[allow(clippy::cast_precision_loss)]
    let v = days_from_civil(y, 1, 1) as f64 * MS_PER_DAY;
    v
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Interval {
    Millisecond,
    Second(u32),
    Minute(u32),
    Hour(u32),
    Day(u32),
    Week,
    Month(u32),
    Year(f64),
}

/// `interval.range(start, stop)`: ceil start, step while < stop.
#[allow(clippy::cast_possible_truncation)]
fn interval_range(interval: Interval, start: f64, stop: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut t = interval_ceil(interval, start);
    while t < stop {
        out.push(t);
        t = interval_step(interval, t);
    }
    out
}

fn interval_floor(interval: Interval, t: f64) -> f64 {
    match interval {
        Interval::Millisecond => t,
        Interval::Second(k) => (t / (1000.0 * f64::from(k))).floor() * 1000.0 * f64::from(k),
        Interval::Minute(k) => (t / (60_000.0 * f64::from(k))).floor() * 60_000.0 * f64::from(k),
        Interval::Hour(k) => {
            (t / (3_600_000.0 * f64::from(k))).floor() * 3_600_000.0 * f64::from(k)
        }
        Interval::Day(k) => {
            let d = floor_day(t);
            if k == 1 {
                d
            } else {
                // timeDay.every(k) filters on (date-of-month - 1) % k == 0.
                let mut dd = d;
                loop {
                    #[allow(clippy::cast_possible_truncation)]
                    let (_, _, dom) = civil_from_days((dd / MS_PER_DAY) as i64);
                    if (dom - 1).rem_euclid(i64::from(k)) == 0 {
                        break;
                    }
                    dd -= MS_PER_DAY;
                }
                dd
            }
        }
        Interval::Week => floor_week(t),
        Interval::Month(k) => {
            let m = floor_month(t);
            if k == 1 {
                m
            } else {
                let (y, mo, _) = civil_from_days((m / MS_PER_DAY) as i64);
                let months = y * 12 + (mo - 1);
                let rem = months.rem_euclid(i64::from(k));
                let months = months - rem;
                #[allow(clippy::cast_precision_loss)]
                let v = days_from_civil(months.div_euclid(12), months.rem_euclid(12) + 1, 1) as f64
                    * MS_PER_DAY;
                v
            }
        }
        Interval::Year(k) => {
            let y0 = floor_year(t);
            #[allow(clippy::cast_possible_truncation)]
            let (y, _, _) = civil_from_days((y0 / MS_PER_DAY) as i64);
            #[allow(clippy::cast_possible_truncation)]
            let k = k as i64;
            let y = if k > 1 { y - y.rem_euclid(k) } else { y };
            #[allow(clippy::cast_precision_loss)]
            let v = days_from_civil(y, 1, 1) as f64 * MS_PER_DAY;
            v
        }
    }
}

fn interval_step(interval: Interval, t: f64) -> f64 {
    match interval {
        Interval::Millisecond => t + 1.0,
        Interval::Second(k) => t + 1000.0 * f64::from(k),
        Interval::Minute(k) => t + 60_000.0 * f64::from(k),
        Interval::Hour(k) => t + 3_600_000.0 * f64::from(k),
        Interval::Day(k) => {
            if k == 1 {
                t + MS_PER_DAY
            } else {
                // Advance day by day to the next (dom-1) % k == 0.
                let mut dd = t + MS_PER_DAY;
                loop {
                    #[allow(clippy::cast_possible_truncation)]
                    let (_, _, dom) = civil_from_days((dd / MS_PER_DAY) as i64);
                    if (dom - 1).rem_euclid(i64::from(k)) == 0 {
                        break;
                    }
                    dd += MS_PER_DAY;
                }
                dd
            }
        }
        Interval::Week => t + 7.0 * MS_PER_DAY,
        Interval::Month(k) => Ts(t).add_months(i64::from(k)).0,
        Interval::Year(k) => {
            #[allow(clippy::cast_possible_truncation)]
            let k = k as i64;
            Ts(t).add_months(12 * k.max(1)).0
        }
    }
}

fn interval_ceil(interval: Interval, t: f64) -> f64 {
    let f = interval_floor(interval, t);
    if f < t { interval_step(interval, f) } else { f }
}

/// d3-scale time `tickInterval(count)` for the default count 10.
fn time_tick_interval(start: f64, stop: f64, count: f64) -> Interval {
    const DURATION_SECOND: f64 = 1000.0;
    const DURATION_MINUTE: f64 = 60_000.0;
    const DURATION_HOUR: f64 = 3_600_000.0;
    const DURATION_DAY: f64 = MS_PER_DAY;
    const DURATION_WEEK: f64 = 7.0 * MS_PER_DAY;
    const DURATION_MONTH: f64 = 2_592_000_000.0;
    const DURATION_YEAR: f64 = 31_536_000_000.0;
    let tick_intervals: [(Interval, f64); 18] = [
        (Interval::Second(1), DURATION_SECOND),
        (Interval::Second(5), 5.0 * DURATION_SECOND),
        (Interval::Second(15), 15.0 * DURATION_SECOND),
        (Interval::Second(30), 30.0 * DURATION_SECOND),
        (Interval::Minute(1), DURATION_MINUTE),
        (Interval::Minute(5), 5.0 * DURATION_MINUTE),
        (Interval::Minute(15), 15.0 * DURATION_MINUTE),
        (Interval::Minute(30), 30.0 * DURATION_MINUTE),
        (Interval::Hour(1), DURATION_HOUR),
        (Interval::Hour(3), 3.0 * DURATION_HOUR),
        (Interval::Hour(6), 6.0 * DURATION_HOUR),
        (Interval::Hour(12), 12.0 * DURATION_HOUR),
        (Interval::Day(1), DURATION_DAY),
        (Interval::Day(2), 2.0 * DURATION_DAY),
        (Interval::Week, DURATION_WEEK),
        (Interval::Month(1), DURATION_MONTH),
        (Interval::Month(3), 3.0 * DURATION_MONTH),
        (Interval::Year(1.0), DURATION_YEAR),
    ];
    let target = (stop - start).abs() / count;
    // bisector right on step
    let mut i = tick_intervals.len();
    for (idx, (_, step)) in tick_intervals.iter().enumerate() {
        if target < *step {
            i = idx;
            break;
        }
    }
    if i == tick_intervals.len() {
        // year.every(tickStep(start/year, stop/year, count))
        let step = d3_tick_step(start / DURATION_YEAR, stop / DURATION_YEAR, count);
        return Interval::Year(step);
    }
    if i == 0 {
        return Interval::Millisecond;
    }
    let pick = if target / tick_intervals[i - 1].1 < tick_intervals[i].1 / target {
        i - 1
    } else {
        i
    };
    tick_intervals[pick].0
}

fn d3_tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let step0 = (stop - start).abs() / count.max(0.0);
    let step1 = 10.0f64.powf((step0.ln() / std::f64::consts::LN_10).floor());
    let error = step0 / step1;
    let step1 = if error >= 50.0f64.sqrt() {
        step1 * 10.0
    } else if error >= 10.0f64.sqrt() {
        step1 * 5.0
    } else if error >= 2.0f64.sqrt() {
        step1 * 2.0
    } else {
        step1
    };
    if stop < start { -step1 } else { step1 }
}

/// d3 `timeFormat` subset.
fn time_format(fmt: &str, t: f64) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let day = (t / MS_PER_DAY).floor() as i64;
    let (y, m, d) = civil_from_days(day);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ms_in_day = (t - {
        #[allow(clippy::cast_precision_loss)]
        let v = day as f64 * MS_PER_DAY;
        v
    }) as i64;
    let hh = ms_in_day / 3_600_000;
    let mm = (ms_in_day / 60_000) % 60;
    let ss = (ms_in_day / 1000) % 60;
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&format!("{y:04}")),
            Some('m') => out.push_str(&format!("{m:02}")),
            Some('d') => out.push_str(&format!("{d:02}")),
            Some('H') => out.push_str(&format!("{hh:02}")),
            Some('M') => out.push_str(&format!("{mm:02}")),
            Some('S') => out.push_str(&format!("{ss:02}")),
            Some('b') => out.push_str(
                [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec",
                ][usize::try_from(m - 1).unwrap_or(0)],
            ),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

// --------------------------------------------------------------- renderer --

const BAR_HEIGHT: f64 = 20.0;
const BAR_GAP: f64 = 4.0;
const TOP_PADDING: f64 = 50.0;
const LEFT_PADDING: f64 = 75.0;
const RIGHT_PADDING: f64 = 75.0;
const GRID_LINE_START_PADDING: f64 = 35.0;
const FONT_SIZE: f64 = 11.0;
const SECTION_FONT_SIZE: f64 = 11.0;
const NUMBER_SECTION_STYLES: usize = 4;
const TITLE_TOP_MARGIN: f64 = 25.0;
const PAGE_WIDTH: f64 = 784.0; // body offsetWidth in the mmdc page

/// SVG whitespace collapse (`xml:space` default) for `getBBox` measurement.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Renders gantt source to a complete SVG document string.
///
/// # Errors
/// Returns a [`GanttParseError`] when the source is not a valid gantt chart.
#[allow(clippy::too_many_lines)]
pub fn render_gantt(source: &str, id: &str) -> Result<String, GanttParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let measurer = crate::text::TextMeasurer::new();

    let w = PAGE_WIDTH;
    #[allow(clippy::cast_precision_loss)]
    let h = 2.0f64.mul_add(TOP_PADDING, db.tasks.len() as f64 * (BAR_HEIGHT + BAR_GAP));

    // Categories in order of first appearance.
    let mut categories: Vec<String> = Vec::new();
    for t in &db.tasks {
        if !categories.contains(&t.section) {
            categories.push(t.section.clone());
        }
    }
    let sec_num = |section: &str| -> usize {
        categories.iter().position(|c| c == section).unwrap_or(0) % NUMBER_SECTION_STYLES
    };

    // Time scale (rangeRound).
    let min_time = db
        .tasks
        .iter()
        .map(|t| t.start.0)
        .fold(f64::INFINITY, f64::min);
    let max_time = db
        .tasks
        .iter()
        .map(|t| t.end.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let range1 = w - LEFT_PADDING - RIGHT_PADDING;
    let scale = |t: f64| -> f64 {
        let tt = (t - min_time) / (max_time - min_time);
        (range1 * tt).round()
    };

    // Tasks sorted by start time (stable).
    let mut sorted: Vec<Task> = db.tasks.clone();
    sorted.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(&svg, "viewBox", format!("0 0 {} {}", js_num(w), js_num(h)));
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(w)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "gantt");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_gantt_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    // --- makeGrid (d3 axisBottom) ---
    {
        let grid = append(&svg, "g");
        set_attr(&grid, "class", "grid");
        set_attr(
            &grid,
            "transform",
            format!("translate({}, {})", js_num(LEFT_PADDING), js_num(h - 50.0)),
        );
        set_attr(&grid, "fill", "none");
        set_attr(&grid, "font-size", "10");
        set_attr(&grid, "font-family", "sans-serif");
        set_attr(&grid, "text-anchor", "middle");

        let axis_format = if db.axis_format.is_empty() {
            "%Y-%m-%d"
        } else {
            &db.axis_format
        };
        let tick_size = -h + TOP_PADDING + GRID_LINE_START_PADDING;
        let offset = 0.5;
        let range0 = 0.0;
        let domain_path = append(&grid, "path");
        set_attr(&domain_path, "class", "domain");
        set_attr(&domain_path, "stroke", "currentColor");
        set_attr(
            &domain_path,
            "d",
            format!(
                "M{},{}V{}H{}V{}",
                js_num(range0 + offset),
                js_num(tick_size),
                js_num(offset),
                js_num(range1 + offset),
                js_num(tick_size)
            ),
        );

        let interval = time_tick_interval(min_time, max_time, 10.0);
        let ticks = interval_range(interval, min_time, max_time + 1.0);
        for tick in ticks {
            let g = append(&grid, "g");
            set_attr(&g, "class", "tick");
            set_attr(&g, "opacity", "1");
            set_attr(
                &g,
                "transform",
                format!("translate({},0)", js_num(scale(tick) + offset)),
            );
            let line = append(&g, "line");
            set_attr(&line, "stroke", "currentColor");
            set_attr(&line, "y2", js_num(tick_size));
            let text = append(&g, "text");
            set_attr(&text, "fill", "#000");
            set_attr(&text, "y", "3");
            set_attr(&text, "dy", "1em");
            set_attr(&text, "stroke", "none");
            set_attr(&text, "font-size", "10");
            set_attr(&text, "style", "text-anchor: middle;");
            set_text(&text, &time_format(axis_format, tick));
        }
    }

    // --- drawRects: section background rows ---
    let gap = BAR_HEIGHT + BAR_GAP;
    {
        let g = append(&svg, "g");
        // uniqueTasks by order over the sorted array.
        let mut seen: Vec<usize> = Vec::new();
        for t in &sorted {
            if seen.contains(&t.order) {
                continue;
            }
            seen.push(t.order);
            let rect = append(&g, "rect");
            set_attr(&rect, "x", "0");
            #[allow(clippy::cast_precision_loss)]
            let y = (t.order as f64).mul_add(gap, TOP_PADDING) - 2.0;
            set_attr(&rect, "y", js_num(y));
            set_attr(&rect, "width", js_num(w - RIGHT_PADDING / 2.0));
            set_attr(&rect, "height", js_num(gap));
            set_attr(
                &rect,
                "class",
                format!("section section{}", sec_num(&t.section)),
            );
        }
    }

    // --- task rects + texts ---
    {
        let g = append(&svg, "g");
        for t in &sorted {
            let rect = append(&g, "rect");
            set_attr(&rect, "id", format!("{id}-{}", t.id));
            set_attr(&rect, "rx", "3");
            set_attr(&rect, "ry", "3");
            let sx = scale(t.start.0);
            let ex = scale(t.end.0);
            let x = if t.milestone {
                0.5f64.mul_add(ex - sx, sx) + LEFT_PADDING - 0.5 * BAR_HEIGHT
            } else {
                sx + LEFT_PADDING
            };
            set_attr(&rect, "x", js_num(x));
            #[allow(clippy::cast_precision_loss)]
            let y = (t.order as f64).mul_add(gap, TOP_PADDING);
            set_attr(&rect, "y", js_num(y));
            set_attr(
                &rect,
                "width",
                js_num(if t.milestone { BAR_HEIGHT } else { ex - sx }),
            );
            set_attr(&rect, "height", js_num(BAR_HEIGHT));
            set_attr(
                &rect,
                "transform-origin",
                format!(
                    "{}px {}px",
                    js_num(0.5f64.mul_add(ex - sx, sx) + LEFT_PADDING),
                    js_num(0.5f64.mul_add(BAR_HEIGHT, y))
                ),
            );
            let sn = sec_num(&t.section);
            let mut task_class = String::new();
            if t.active {
                if t.crit {
                    task_class += " activeCrit";
                } else {
                    task_class = " active".to_owned();
                }
            } else if t.done {
                if t.crit {
                    task_class = " doneCrit".to_owned();
                } else {
                    task_class = " done".to_owned();
                }
            } else if t.crit {
                task_class += " crit";
            }
            if task_class.is_empty() {
                task_class = " task".to_owned();
            }
            if t.milestone {
                task_class = format!(" milestone {task_class}");
            }
            set_attr(&rect, "class", format!("task{task_class}{sn} "));
        }

        for t in &sorted {
            let text = append(&g, "text");
            set_attr(&text, "id", format!("{id}-{}-text", t.id));
            set_attr(&text, "font-size", js_num(FONT_SIZE));
            let sx = scale(t.start.0);
            let ex = scale(t.end.0);
            let (start_x, end_x) = if t.milestone {
                let s = sx + 0.5f64.mul_add(ex - sx, -(0.5 * BAR_HEIGHT));
                (s, s + BAR_HEIGHT)
            } else {
                (sx, ex)
            };
            let text_width = measurer.measure_width(&collapse_ws(&t.name), FONT_SIZE);
            let x = if text_width > end_x - start_x {
                if 1.5f64.mul_add(LEFT_PADDING, end_x + text_width) > w {
                    start_x + LEFT_PADDING - 5.0
                } else {
                    end_x + LEFT_PADDING + 5.0
                }
            } else {
                (end_x - start_x) / 2.0 + start_x + LEFT_PADDING
            };
            set_attr(&text, "x", js_num(x));
            #[allow(clippy::cast_precision_loss)]
            let y = (t.order as f64).mul_add(gap, BAR_HEIGHT / 2.0 + (FONT_SIZE / 2.0 - 2.0))
                + TOP_PADDING;
            set_attr(&text, "y", js_num(y));
            // (the upstream text-height attribute is stripped by DOMPurify)

            let sn = sec_num(&t.section);
            let mut task_type = String::new();
            if t.active {
                task_type = if t.crit {
                    format!("activeCritText{sn}")
                } else {
                    format!("activeText{sn}")
                };
            }
            if t.done {
                if t.crit {
                    task_type = format!("{task_type} doneCritText{sn}");
                } else {
                    task_type = format!("{task_type} doneText{sn}");
                }
            } else if t.crit {
                task_type = format!("{task_type} critText{sn}");
            }
            if t.milestone {
                task_type += " milestoneText";
            }
            let class = if text_width > end_x - start_x {
                if 1.5f64.mul_add(LEFT_PADDING, end_x + text_width) > w {
                    format!(" taskTextOutsideLeft taskTextOutside{sn} {task_type}")
                } else {
                    format!(
                        " taskTextOutsideRight taskTextOutside{sn} {task_type} width-{}",
                        js_num(text_width)
                    )
                }
            } else {
                format!(
                    " taskText taskText{sn} {task_type} width-{}",
                    js_num(text_width)
                )
            };
            set_attr(&text, "class", class);
            set_text(&text, &t.name);
        }
    }

    // --- vertLabels (section titles) ---
    {
        let g = append(&svg, "g");
        let mut prev_gap = 0.0f64;
        let num_occurrences: Vec<(String, f64)> = categories
            .iter()
            .map(|c| {
                #[allow(clippy::cast_precision_loss)]
                let n = db.tasks.iter().filter(|t| t.section == *c).count() as f64;
                (c.clone(), n)
            })
            .collect();
        for (i, (name, n)) in num_occurrences.iter().enumerate() {
            let text = append(&g, "text");
            set_attr(&text, "dy", "0em");
            set_attr(&text, "x", "10");
            let y = if i > 0 {
                prev_gap += num_occurrences[i - 1].1;
                (n * gap) / 2.0 + prev_gap * gap + TOP_PADDING
            } else {
                (n * gap) / 2.0 + TOP_PADDING
            };
            set_attr(&text, "y", js_num(y));
            set_attr(&text, "font-size", js_num(SECTION_FONT_SIZE));
            set_attr(
                &text,
                "class",
                format!(
                    "sectionTitle sectionTitle{}",
                    categories.iter().position(|c| c == name).unwrap_or(0) % NUMBER_SECTION_STYLES
                ),
            );
            let tspan = append(&text, "tspan");
            set_attr(&tspan, "alignment-baseline", "central");
            set_attr(&tspan, "x", "10");
            set_text(&tspan, name);
        }
    }

    // --- today marker ---
    {
        let g = append(&svg, "g");
        set_attr(&g, "class", "today");
        let line = append(&g, "line");
        let now_ms = {
            #[allow(clippy::cast_precision_loss)]
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as f64)
                .unwrap_or(0.0);
            ms + local_utc_offset_ms()
        };
        let x = scale(now_ms) + LEFT_PADDING;
        set_attr(&line, "x1", js_num(x));
        set_attr(&line, "x2", js_num(x));
        set_attr(&line, "y1", js_num(TITLE_TOP_MARGIN));
        set_attr(&line, "y2", js_num(h - TITLE_TOP_MARGIN));
        set_attr(&line, "class", "today");
    }

    // --- title ---
    let title = append(&svg, "text");
    if !db.title.is_empty() {
        set_text(&title, &db.title);
    }
    set_attr(&title, "x", js_num(w / 2.0));
    set_attr(&title, "y", js_num(TITLE_TOP_MARGIN));
    set_attr(&title, "class", "titleText");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// Local timezone offset in milliseconds (from the `TZ`-aware `date`
/// output); today-marker positions are masked in tests, so precision is
/// not critical.
fn local_utc_offset_ms() -> f64 {
    use std::sync::OnceLock;
    static OFFSET: OnceLock<f64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                if s.len() < 5 {
                    return None;
                }
                let sign = if s.starts_with('-') { -1.0 } else { 1.0 };
                let hh: f64 = s[1..3].parse().ok()?;
                let mm: f64 = s[3..5].parse().ok()?;
                Some(sign * hh.mul_add(60.0, mm) * 60_000.0)
            })
            .unwrap_or(0.0)
    })
}
