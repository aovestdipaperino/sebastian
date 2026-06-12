//! Port of the khroma color library (v2.1.0) as used by mermaid's themes.
//!
//! Faithful to khroma's lazy channel conversion, clamping, and stringify
//! rules, since theme variables embed its exact output strings.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelsType {
    All,
    Rgb,
    Hsl,
}

#[derive(Debug, Clone)]
pub struct Channels {
    r: Option<f64>,
    g: Option<f64>,
    b: Option<f64>,
    h: Option<f64>,
    s: Option<f64>,
    l: Option<f64>,
    a: f64,
    ty: ChannelsType,
    changed: bool,
    color: Option<String>,
}

fn round10(n: f64) -> f64 {
    (n * 1e10).round() / 1e10
}

mod clamp {
    pub fn r(v: f64) -> f64 {
        v.clamp(0.0, 255.0)
    }
    pub fn g(v: f64) -> f64 {
        r(v)
    }
    pub fn b(v: f64) -> f64 {
        r(v)
    }
    pub fn h(v: f64) -> f64 {
        v % 360.0
    }
    pub fn s(v: f64) -> f64 {
        v.clamp(0.0, 100.0)
    }
    pub fn l(v: f64) -> f64 {
        s(v)
    }
    pub fn a(v: f64) -> f64 {
        v.clamp(0.0, 1.0)
    }
}

fn hue2rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsl2rgb(h: f64, s: f64, l: f64, channel: char) -> f64 {
    if s == 0.0 {
        return l * 2.55; // Achromatic
    }
    let h = h / 360.0;
    let s = s / 100.0;
    let l = l / 100.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        (l + s) - (l * s)
    };
    let p = 2.0 * l - q;
    match channel {
        'r' => hue2rgb(p, q, h + 1.0 / 3.0) * 255.0,
        'g' => hue2rgb(p, q, h) * 255.0,
        _ => hue2rgb(p, q, h - 1.0 / 3.0) * 255.0,
    }
}

fn rgb2hsl(r: f64, g: f64, b: f64, channel: char) -> f64 {
    let r = r / 255.0;
    let g = g / 255.0;
    let b = b / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = f64::midpoint(max, min);
    if channel == 'l' {
        return l * 100.0;
    }
    if max == min {
        return 0.0; // Achromatic
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    if channel == 's' {
        return s * 100.0;
    }
    if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if max == g {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    }
}

impl Channels {
    fn new() -> Self {
        Self {
            r: None,
            g: None,
            b: None,
            h: None,
            s: None,
            l: None,
            a: 1.0,
            ty: ChannelsType::All,
            changed: false,
            color: None,
        }
    }

    fn ensure_hsl(&mut self) {
        // Fields are computed only when missing; sources exist in that case.
        if self.h.is_none() || self.s.is_none() || self.l.is_none() {
            let (r, g, b) = (
                self.r.expect("rgb"),
                self.g.expect("rgb"),
                self.b.expect("rgb"),
            );
            if self.h.is_none() {
                self.h = Some(rgb2hsl(r, g, b, 'h'));
            }
            if self.s.is_none() {
                self.s = Some(rgb2hsl(r, g, b, 's'));
            }
            if self.l.is_none() {
                self.l = Some(rgb2hsl(r, g, b, 'l'));
            }
        }
    }

    fn ensure_rgb(&mut self) {
        if self.r.is_none() || self.g.is_none() || self.b.is_none() {
            let (h, s, l) = (
                self.h.expect("hsl"),
                self.s.expect("hsl"),
                self.l.expect("hsl"),
            );
            if self.r.is_none() {
                self.r = Some(hsl2rgb(h, s, l, 'r'));
            }
            if self.g.is_none() {
                self.g = Some(hsl2rgb(h, s, l, 'g'));
            }
            if self.b.is_none() {
                self.b = Some(hsl2rgb(h, s, l, 'b'));
            }
        }
    }

    pub fn get_r(&mut self) -> f64 {
        if self.ty != ChannelsType::Hsl
            && let Some(r) = self.r
        {
            return r;
        }
        self.ensure_hsl();
        hsl2rgb(
            self.h.expect("h"),
            self.s.expect("s"),
            self.l.expect("l"),
            'r',
        )
    }

    pub fn get_g(&mut self) -> f64 {
        if self.ty != ChannelsType::Hsl
            && let Some(g) = self.g
        {
            return g;
        }
        self.ensure_hsl();
        hsl2rgb(
            self.h.expect("h"),
            self.s.expect("s"),
            self.l.expect("l"),
            'g',
        )
    }

    pub fn get_b(&mut self) -> f64 {
        if self.ty != ChannelsType::Hsl
            && let Some(b) = self.b
        {
            return b;
        }
        self.ensure_hsl();
        hsl2rgb(
            self.h.expect("h"),
            self.s.expect("s"),
            self.l.expect("l"),
            'b',
        )
    }

    pub fn get_h(&mut self) -> f64 {
        if self.ty != ChannelsType::Rgb
            && let Some(h) = self.h
        {
            return h;
        }
        self.ensure_rgb();
        rgb2hsl(
            self.r.expect("r"),
            self.g.expect("g"),
            self.b.expect("b"),
            'h',
        )
    }

    pub fn get_s(&mut self) -> f64 {
        if self.ty != ChannelsType::Rgb
            && let Some(s) = self.s
        {
            return s;
        }
        self.ensure_rgb();
        rgb2hsl(
            self.r.expect("r"),
            self.g.expect("g"),
            self.b.expect("b"),
            's',
        )
    }

    pub fn get_l(&mut self) -> f64 {
        if self.ty != ChannelsType::Rgb
            && let Some(l) = self.l
        {
            return l;
        }
        self.ensure_rgb();
        rgb2hsl(
            self.r.expect("r"),
            self.g.expect("g"),
            self.b.expect("b"),
            'l',
        )
    }

    pub fn get(&mut self, channel: char) -> f64 {
        match channel {
            'r' => self.get_r(),
            'g' => self.get_g(),
            'b' => self.get_b(),
            'h' => self.get_h(),
            's' => self.get_s(),
            'l' => self.get_l(),
            _ => self.a,
        }
    }

    fn set(&mut self, channel: char, value: f64) {
        self.changed = true;
        match channel {
            'r' => {
                self.ty = ChannelsType::Rgb;
                self.r = Some(value);
            }
            'g' => {
                self.ty = ChannelsType::Rgb;
                self.g = Some(value);
            }
            'b' => {
                self.ty = ChannelsType::Rgb;
                self.b = Some(value);
            }
            'h' => {
                self.ty = ChannelsType::Hsl;
                self.h = Some(value);
            }
            's' => {
                self.ty = ChannelsType::Hsl;
                self.s = Some(value);
            }
            'l' => {
                self.ty = ChannelsType::Hsl;
                self.l = Some(value);
            }
            _ => self.a = value,
        }
    }
}

fn clamp_channel(channel: char, value: f64) -> f64 {
    match channel {
        'r' => clamp::r(value),
        'g' => clamp::g(value),
        'b' => clamp::b(value),
        'h' => clamp::h(value),
        's' => clamp::s(value),
        'l' => clamp::l(value),
        _ => clamp::a(value),
    }
}

/// CSS color keywords used in mermaid themes (subset of the full table).
fn keyword(color: &str) -> Option<(f64, f64, f64)> {
    Some(match color {
        "black" => (0.0, 0.0, 0.0),
        "white" => (255.0, 255.0, 255.0),
        "red" => (255.0, 0.0, 0.0),
        "green" => (0.0, 128.0, 0.0),
        "blue" => (0.0, 0.0, 255.0),
        "yellow" => (255.0, 255.0, 0.0),
        "grey" | "gray" => (128.0, 128.0, 128.0),
        "lightgrey" | "lightgray" => (211.0, 211.0, 211.0),
        "darkgrey" | "darkgray" => (169.0, 169.0, 169.0),
        "orange" => (255.0, 165.0, 0.0),
        "purple" => (128.0, 0.0, 128.0),
        "pink" => (255.0, 192.0, 203.0),
        "brown" => (165.0, 42.0, 42.0),
        "cyan" | "aqua" => (0.0, 255.0, 255.0),
        "magenta" | "fuchsia" => (255.0, 0.0, 255.0),
        "lime" => (0.0, 255.0, 0.0),
        "navy" => (0.0, 0.0, 128.0),
        "teal" => (0.0, 128.0, 128.0),
        "olive" => (128.0, 128.0, 0.0),
        "maroon" => (128.0, 0.0, 0.0),
        "silver" => (192.0, 192.0, 192.0),
        // "transparent" also lands here; the caller handles it via alpha.
        _ => return None,
    })
}

/// Parses a color string into channels (hex, rgb(a), hsl(a), keyword).
pub fn parse(color: &str) -> Channels {
    let mut ch = Channels::new();
    ch.color = Some(color.to_owned());

    // Hex
    if let Some(hex) = color.strip_prefix('#') {
        let len = hex.len();
        if (len == 3 || len == 4 || len == 6 || len == 8)
            && hex.chars().all(|c| c.is_ascii_hexdigit())
            && let Ok(dec) = u32::from_str_radix(hex, 16)
        {
            let has_alpha = len % 4 == 0;
            let full = len > 4;
            let mult = if full { 1.0 } else { 17.0 };
            let bits = if full { 8 } else { 4 };
            let offset: i32 = if has_alpha { 0 } else { -1 };
            let mask: u32 = if full { 255 } else { 15 };
            let part = |idx: i32| -> f64 {
                let shift = bits * (offset + idx);
                f64::from((dec >> shift) & mask) * mult
            };
            ch.r = Some(part(3));
            ch.g = Some(part(2));
            ch.b = Some(part(1));
            ch.a = if has_alpha {
                f64::from(dec & mask) * mult / 255.0
            } else {
                1.0
            };
            return ch;
        }
    }

    // rgb()/rgba()
    if (color.starts_with("rgb") || color.starts_with("RGB"))
        && let Some(inner) = color
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner
            .split([',', '/'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() >= 3 {
            let num = |s: &str| -> Option<(f64, bool)> {
                let pct = s.ends_with('%');
                s.trim_end_matches('%').parse().ok().map(|v| (v, pct))
            };
            if let (Some((r, rp)), Some((g, gp)), Some((b, bp))) =
                (num(parts[0]), num(parts[1]), num(parts[2]))
            {
                ch.r = Some(clamp::r(if rp { r * 2.55 } else { r }));
                ch.g = Some(clamp::g(if gp { g * 2.55 } else { g }));
                ch.b = Some(clamp::b(if bp { b * 2.55 } else { b }));
                ch.a = parts
                    .get(3)
                    .and_then(|s| num(s))
                    .map_or(1.0, |(a, ap)| clamp::a(if ap { a / 100.0 } else { a }));
                return ch;
            }
        }
    }

    // hsl()/hsla()
    if (color.starts_with("hsl") || color.starts_with("HSL"))
        && let Some(inner) = color
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner
            .split([',', '/'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() >= 3 {
            let h: Option<f64> = parts[0].trim_end_matches("deg").parse().ok();
            let s: Option<f64> = parts[1].trim_end_matches('%').parse().ok();
            let l: Option<f64> = parts[2].trim_end_matches('%').parse().ok();
            if let (Some(h), Some(s), Some(l)) = (h, s, l) {
                ch.h = Some(clamp::h(h));
                ch.s = Some(clamp::s(s));
                ch.l = Some(clamp::l(l));
                ch.a = parts
                    .get(3)
                    .and_then(|x| x.parse().ok())
                    .map_or(1.0, clamp::a);
                return ch;
            }
        }
    }

    // Keyword
    if color == "transparent" {
        ch.r = Some(0.0);
        ch.g = Some(0.0);
        ch.b = Some(0.0);
        ch.a = 0.0;
        return ch;
    }
    if let Some((r, g, b)) = keyword(&color.to_lowercase()) {
        ch.r = Some(r);
        ch.g = Some(g);
        ch.b = Some(b);
        return ch;
    }

    panic!("unsupported color format: {color:?}");
}

fn dec2hex(v: f64) -> String {
    format!("{:02x}", v.round() as u32)
}

/// khroma `Color.stringify`.
pub fn stringify(ch: &mut Channels) -> String {
    if !ch.changed
        && let Some(color) = &ch.color
    {
        return color.clone();
    }
    if ch.ty == ChannelsType::Hsl || ch.r.is_none() {
        let (h, s, l, a) = (ch.get_h(), ch.get_s(), ch.get_l(), ch.a);
        if a < 1.0 {
            return format!(
                "hsla({}, {}%, {}%, {})",
                fmt(round10(h)),
                fmt(round10(s)),
                fmt(round10(l)),
                fmt(a)
            );
        }
        return format!(
            "hsl({}, {}%, {}%)",
            fmt(round10(h)),
            fmt(round10(s)),
            fmt(round10(l))
        );
    }
    let (r, g, b, a) = (ch.get_r(), ch.get_g(), ch.get_b(), ch.a);
    if a < 1.0 || r.fract() != 0.0 || g.fract() != 0.0 || b.fract() != 0.0 {
        if a < 1.0 {
            return format!(
                "rgba({}, {}, {}, {})",
                fmt(round10(r)),
                fmt(round10(g)),
                fmt(round10(b)),
                fmt(round10(a))
            );
        }
        return format!(
            "rgb({}, {}, {})",
            fmt(round10(r)),
            fmt(round10(g)),
            fmt(round10(b))
        );
    }
    format!("#{}{}{}", dec2hex(r), dec2hex(g), dec2hex(b))
}

/// JS number formatting for embedded values.
fn fmt(n: f64) -> String {
    crate::svg::js_num(n)
}

/// khroma `change`.
#[must_use]
pub fn change(color: &str, changes: &[(char, f64)]) -> String {
    let mut ch = parse(color);
    for &(c, v) in changes {
        ch.set(c, clamp_channel(c, v));
    }
    stringify(&mut ch)
}

/// khroma `adjust`.
#[must_use]
pub fn adjust(color: &str, deltas: &[(char, f64)]) -> String {
    let mut ch = parse(color);
    let mut changes: Vec<(char, f64)> = Vec::new();
    for &(c, delta) in deltas {
        if delta == 0.0 {
            continue; // JS skips falsy deltas
        }
        changes.push((c, ch.get(c) + delta));
    }
    change(color, &changes)
}

/// khroma `adjustChannel` (lighten/darken).
fn adjust_channel(color: &str, channel: char, amount: f64) -> String {
    let mut ch = parse(color);
    let current = ch.get(channel);
    let next = clamp_channel(channel, current + amount);
    if current != next {
        ch.set(channel, next);
    }
    stringify(&mut ch)
}

#[must_use]
pub fn lighten(color: &str, amount: f64) -> String {
    adjust_channel(color, 'l', amount)
}

#[must_use]
pub fn darken(color: &str, amount: f64) -> String {
    adjust_channel(color, 'l', -amount)
}

/// khroma `mix` (SASS-compatible).
pub fn mix(c1: &mut Channels, c2: &mut Channels, weight: f64) -> String {
    let (r1, g1, b1, a1) = (c1.get_r(), c1.get_g(), c1.get_b(), c1.a);
    let (r2, g2, b2, a2) = (c2.get_r(), c2.get_g(), c2.get_b(), c2.a);
    let weight_scale = weight / 100.0;
    let weight_normalized = weight_scale * 2.0 - 1.0;
    let alpha_delta = a1 - a2;
    let weight1_combined = if weight_normalized * alpha_delta == -1.0 {
        weight_normalized
    } else {
        (weight_normalized + alpha_delta) / (1.0 + weight_normalized * alpha_delta)
    };
    let weight1 = f64::midpoint(weight1_combined, 1.0);
    let weight2 = 1.0 - weight1;
    rgba(
        r1 * weight1 + r2 * weight2,
        g1 * weight1 + g2 * weight2,
        b1 * weight1 + b2 * weight2,
        a1 * weight_scale + a2 * (1.0 - weight_scale),
    )
}

/// khroma `invert`.
#[must_use]
pub fn invert(color: &str) -> String {
    let mut inverse = parse(color);
    let (r, g, b) = (inverse.get_r(), inverse.get_g(), inverse.get_b());
    inverse.set('r', 255.0 - r);
    inverse.set('g', 255.0 - g);
    inverse.set('b', 255.0 - b);
    let mut original = parse(color);
    mix(&mut inverse, &mut original, 100.0)
}

/// khroma `rgba(r, g, b, a)`.
#[must_use]
pub fn rgba(r: f64, g: f64, b: f64, a: f64) -> String {
    let mut ch = Channels::new();
    ch.r = Some(clamp::r(r));
    ch.g = Some(clamp::g(g));
    ch.b = Some(clamp::b(b));
    ch.a = clamp::a(a);
    stringify(&mut ch)
}

/// khroma `rgba(color, alpha)` — the two-argument form.
#[must_use]
pub fn rgba_alpha(color: &str, alpha: f64) -> String {
    change(color, &[('a', alpha)])
}

/// khroma `channel(color, c)`.
#[must_use]
pub fn channel(color: &str, c: char) -> f64 {
    round10(parse(color).get(c))
}

fn to_linear(c: f64) -> f64 {
    let n = c / 255.0;
    if c > 0.03928 {
        ((n + 0.055) / 1.055).powf(2.4)
    } else {
        n / 12.92
    }
}

#[must_use]
pub fn luminance(color: &str) -> f64 {
    let mut ch = parse(color);
    let (r, g, b) = (ch.get_r(), ch.get_g(), ch.get_b());
    round10(0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b))
}

#[must_use]
pub fn is_dark(color: &str) -> bool {
    luminance(color) < 0.5
}

/// theme-helpers `mkBorder`.
#[must_use]
pub fn mk_border(color: &str, dark_mode: bool) -> String {
    if dark_mode {
        adjust(color, &[('s', -40.0), ('l', 10.0)])
    } else {
        adjust(color, &[('s', -40.0), ('l', -10.0)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_khroma_outputs() {
        // Ground truth from khroma 2.1.0 in Node.
        assert_eq!(
            adjust("#ECECFF", &[('h', -160.0)]),
            "hsl(80, 100%, 96.2745098039%)"
        );
        assert_eq!(mk_border("#ECECFF", false), "hsl(240, 60%, 86.2745098039%)");
        assert_eq!(invert("#f4f4f4"), "#0b0b0b");
        assert_eq!(rgba(232.0, 232.0, 232.0, 0.5), "rgba(232, 232, 232, 0.5)");
        assert_eq!(channel("#ECECFF", 'r'), 236.0);
        assert_eq!(
            lighten("#1f2020", 16.0),
            "hsl(180, 1.5873015873%, 28.3529411765%)"
        );
        assert_eq!(invert("#1f2020"), "#e0dfdf");
        assert_eq!(
            adjust("#fff4dd", &[('h', -120.0)]),
            "hsl(-79.4117647059, 100%, 93.3333333333%)"
        );
        assert_eq!(
            adjust("#fff4dd", &[('h', 180.0), ('l', 5.0)]),
            "hsl(220.5882352941, 100%, 98.3333333333%)"
        );
        assert_eq!(
            darken("hsl(-79.4117647059, 100%, 93.3333333333%)", 30.0),
            "hsl(-79.4117647059, 100%, 63.3333333333%)"
        );
        assert_eq!(lighten("#181818", 25.0), "hsl(0, 0%, 34.4117647059%)");
        assert_eq!(invert("#333"), "#cccccc");
        assert_eq!(invert("green"), "#ff7fff");
        assert_eq!(invert("white"), "#000000");
    }
}
