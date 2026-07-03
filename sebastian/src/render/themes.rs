//! Port of mermaid's theme classes (base, default, dark, forest) — only the
//! variables consumed by the flowchart stylesheet, computed with the same
//! khroma operations and assignment order as the JS sources.

use serde_json::{Map, Value};

use super::khroma::{adjust, darken, invert, lighten, mk_border, rgba};

type Vars = Map<String, Value>;

fn set(vars: &mut Vars, key: &str, value: impl Into<String>) {
    vars.insert(key.to_owned(), Value::String(value.into()));
}

fn set_bool(vars: &mut Vars, key: &str, value: bool) {
    vars.insert(key.to_owned(), Value::Bool(value));
}

#[must_use]
pub fn get(vars: &Vars, key: &str) -> String {
    match vars.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

#[must_use]
pub fn get_bool(vars: &Vars, key: &str) -> bool {
    match vars.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

/// JS truthiness for the `this.x = this.x || calc()` pattern.
fn unset(vars: &Vars, key: &str) -> bool {
    match vars.get(key) {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Bool(b)) => !*b,
        _ => false,
    }
}

fn set_if(vars: &mut Vars, key: &str, value: impl FnOnce(&Vars) -> String) {
    if unset(vars, key) {
        let v = value(vars);
        vars.insert(key.to_owned(), Value::String(v));
    }
}

fn dark_mode(vars: &Vars) -> bool {
    get_bool(vars, "darkMode")
}

const FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial, sans-serif";

/// `getThemeVariables(overrides)` for the named theme.
pub fn theme_variables(theme: &str, overrides: &Vars) -> Vars {
    let mut vars = Vars::new();
    match theme {
        "base" => {
            ctor_base(&mut vars);
            calculate(&mut vars, overrides, update_base);
        }
        "dark" => {
            ctor_dark(&mut vars);
            calculate(&mut vars, overrides, update_dark);
        }
        "forest" => {
            ctor_forest(&mut vars);
            calculate(&mut vars, overrides, update_forest);
        }
        _ => {
            // default (and the fallback for unknown themes via config.ts)
            ctor_default(&mut vars);
            update_default(&mut vars);
            calculate(&mut vars, overrides, update_default);
        }
    }
    vars
}

fn calculate(vars: &mut Vars, overrides: &Vars, update: fn(&mut Vars)) {
    for (k, v) in overrides {
        vars.insert(k.clone(), v.clone());
    }
    update(vars);
    for (k, v) in overrides {
        vars.insert(k.clone(), v.clone());
    }
}

fn ctor_default(vars: &mut Vars) {
    set(vars, "radius", "5");
    set(vars, "labelTextColor", "calculated");
    set(vars, "THEME_COLOR_LIMIT", "12");
    set(vars, "actorTextColor", "black");
    set(vars, "activationBorderColor", "#666");
    set(vars, "activationBkgColor", "#f4f4f4");
    set(vars, "sequenceNumberColor", "white");
    set(vars, "background", "#f4f4f4");
    set(vars, "primaryColor", "#ECECFF");
    let p = get(vars, "primaryColor");
    set(vars, "secondaryColor", adjust(&p, &[('h', 120.0)]));
    set(vars, "secondaryColor", "#ffffde");
    set(vars, "tertiaryColor", adjust(&p, &[('h', -160.0)]));
    let dm = dark_mode(vars);
    set(vars, "primaryBorderColor", mk_border(&p, dm));
    set(
        vars,
        "secondaryBorderColor",
        mk_border(&get(vars, "secondaryColor"), dm),
    );
    set(
        vars,
        "tertiaryBorderColor",
        mk_border(&get(vars, "tertiaryColor"), dm),
    );
    set(vars, "primaryTextColor", invert(&p));
    set(
        vars,
        "secondaryTextColor",
        invert(&get(vars, "secondaryColor")),
    );
    set(
        vars,
        "tertiaryTextColor",
        invert(&get(vars, "tertiaryColor")),
    );
    set(vars, "lineColor", invert(&get(vars, "background")));
    set(vars, "textColor", invert(&get(vars, "background")));
    set(vars, "background", "white");
    set(vars, "mainBkg", "#ECECFF");
    set(vars, "secondBkg", "#ffffde");
    set(vars, "lineColor", "#333333");
    set(vars, "border1", "#9370DB");
    set(vars, "primaryBorderColor", mk_border(&p, dm));
    set(vars, "border2", "#aaaa33");
    set(vars, "arrowheadColor", "#333333");
    set(vars, "fontFamily", FONT_FAMILY);
    set(vars, "fontSize", "16px");
    set(vars, "labelBackground", "rgba(232,232,232, 0.8)");
    set(vars, "textColor", "#333");
    vars.insert("strokeWidth".to_owned(), Value::from(1));
    set(vars, "errorBkgColor", "#552222");
    set(vars, "errorTextColor", "#552222");
    set_bool(vars, "useGradient", false);
    set(vars, "gradientStart", get(vars, "primaryBorderColor"));
    set(vars, "gradientStop", get(vars, "secondaryBorderColor"));
    set(
        vars,
        "dropShadow",
        "drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))",
    );
}

#[allow(clippy::too_many_lines)]
fn update_default(vars: &mut Vars) {
    // Color scale (cScale0..11) — the unconditional darken runs on every
    // updateColors pass, and mermaid runs two passes.
    set_if(vars, "cScale0", |v| get(v, "primaryColor"));
    set_if(vars, "cScale1", |v| get(v, "secondaryColor"));
    set_if(vars, "cScale2", |v| get(v, "tertiaryColor"));
    for (i, h) in [
        (3, 30.0),
        (4, 60.0),
        (5, 90.0),
        (6, 120.0),
        (7, 150.0),
        (8, 210.0),
        (9, 270.0),
        (10, 300.0),
        (11, 330.0),
    ] {
        set_if(vars, &format!("cScale{i}"), |v| {
            adjust(&get(v, "primaryColor"), &[('h', h)])
        });
    }
    set_if(vars, "cScalePeer1", |v| {
        darken(&get(v, "secondaryColor"), 45.0)
    });
    set_if(vars, "cScalePeer2", |v| {
        darken(&get(v, "tertiaryColor"), 40.0)
    });
    for i in 0..12 {
        let key = format!("cScale{i}");
        let darkened = darken(&get(vars, &key), 10.0);
        set(vars, &key, darkened);
        set_if(vars, &format!("cScalePeer{i}"), |v| {
            darken(&get(v, &format!("cScale{i}")), 25.0)
        });
    }
    for i in 0..12 {
        set_if(vars, &format!("cScaleInv{i}"), |v| {
            adjust(&get(v, &format!("cScale{i}")), &[('h', 180.0)])
        });
    }
    if get(vars, "labelTextColor") != "calculated" {
        set_if(vars, "cScaleLabel0", |v| invert(&get(v, "labelTextColor")));
        set_if(vars, "cScaleLabel3", |v| invert(&get(v, "labelTextColor")));
        for i in 0..12 {
            set_if(vars, &format!("cScaleLabel{i}"), |v| {
                get(v, "labelTextColor")
            });
        }
    }
    set(vars, "nodeBkg", get(vars, "mainBkg"));
    set(vars, "nodeBorder", get(vars, "border1"));
    set(vars, "clusterBkg", get(vars, "secondBkg"));
    set(vars, "clusterBorder", get(vars, "border2"));
    set(vars, "defaultLinkColor", get(vars, "lineColor"));
    set(vars, "titleColor", get(vars, "textColor"));
    set(vars, "edgeLabelBackground", get(vars, "labelBackground"));
    set(vars, "noteBkgColor", "#fff5ad");
    set(vars, "noteBorderColor", get(vars, "border2"));
    set(vars, "noteTextColor", "black");
    set(vars, "actorBorder", get(vars, "border1"));
    set(vars, "actorBkg", get(vars, "mainBkg"));
    set(vars, "labelBoxBkgColor", get(vars, "actorBkg"));
    set(vars, "signalColor", get(vars, "textColor"));
    set(vars, "signalTextColor", get(vars, "textColor"));
    set(vars, "labelBoxBorderColor", get(vars, "actorBorder"));
    set(vars, "labelTextColor", get(vars, "actorTextColor"));
    set(vars, "loopTextColor", get(vars, "actorTextColor"));
    set(vars, "actorLineColor", get(vars, "actorBorder"));
    set_if(vars, "transitionColor", |v| get(v, "lineColor"));
    set_if(vars, "transitionLabelColor", |v| get(v, "textColor"));
    set_if(vars, "stateLabelColor", |v| {
        let sb = get(v, "stateBkg");
        if sb.is_empty() {
            get(v, "primaryTextColor")
        } else {
            sb
        }
    });
    set_if(vars, "stateBkg", |v| get(v, "mainBkg"));
    set_if(vars, "labelBackgroundColor", |v| get(v, "stateBkg"));
    set_if(vars, "compositeBackground", |v| {
        let bg = get(v, "background");
        if bg.is_empty() {
            get(v, "tertiaryColor")
        } else {
            bg
        }
    });
    set_if(vars, "altBackground", |_| "#f0f0f0".to_owned());
    set_if(vars, "compositeTitleBackground", |v| get(v, "mainBkg"));
    set_if(vars, "compositeBorder", |v| get(v, "nodeBorder"));
    set(vars, "innerEndBackground", get(vars, "nodeBorder"));
    set(vars, "specialStateColor", get(vars, "lineColor"));
    set_if(vars, "errorBkgColor", |v| get(v, "tertiaryColor"));
    set_if(vars, "errorTextColor", |v| get(v, "tertiaryTextColor"));
    set(vars, "classText", get(vars, "primaryTextColor"));
    // Git colors (timeline's .section-root uses git0/gitBranchLabel0).
    set_if(vars, "git0", |v| get(v, "primaryColor"));
    set_if(vars, "git1", |v| get(v, "secondaryColor"));
    set_if(vars, "git2", |v| get(v, "tertiaryColor"));
    set_if(vars, "git3", |v| {
        adjust(&get(v, "primaryColor"), &[('h', -30.0)])
    });
    set_if(vars, "git4", |v| {
        adjust(&get(v, "primaryColor"), &[('h', -60.0)])
    });
    set_if(vars, "git5", |v| {
        adjust(&get(v, "primaryColor"), &[('h', -90.0)])
    });
    set_if(vars, "git6", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 60.0)])
    });
    set_if(vars, "git7", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 120.0)])
    });
    for i in 0..8 {
        let key = format!("git{i}");
        let darkened = darken(&get(vars, &key), 25.0);
        set(vars, &key, darkened);
    }
    // ER row colors.
    set_if(vars, "rowOdd", |v| {
        let l = super::khroma::lighten(&get(v, "primaryColor"), 75.0);
        if l.is_empty() {
            "#ffffff".to_owned()
        } else {
            l
        }
    });
    set_if(vars, "rowEven", |v| {
        super::khroma::lighten(&get(v, "primaryColor"), 1.0)
    });
    // Pie colors (theme-default "pie" block); taskTextDarkColor is 'black'.
    set(vars, "taskTextDarkColor", "black");
    set_if(vars, "pie1", |v| get(v, "primaryColor"));
    set_if(vars, "pie2", |v| get(v, "secondaryColor"));
    set_if(vars, "pie3", |v| {
        adjust(&get(v, "tertiaryColor"), &[('l', -40.0)])
    });
    set_if(vars, "pie4", |v| {
        adjust(&get(v, "primaryColor"), &[('l', -10.0)])
    });
    set_if(vars, "pie5", |v| {
        adjust(&get(v, "secondaryColor"), &[('l', -30.0)])
    });
    set_if(vars, "pie6", |v| {
        adjust(&get(v, "tertiaryColor"), &[('l', -20.0)])
    });
    set_if(vars, "pie7", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 60.0), ('l', -20.0)])
    });
    set_if(vars, "pie8", |v| {
        adjust(&get(v, "primaryColor"), &[('h', -60.0), ('l', -40.0)])
    });
    set_if(vars, "pie9", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 120.0), ('l', -40.0)])
    });
    set_if(vars, "pie10", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 60.0), ('l', -40.0)])
    });
    set_if(vars, "pie11", |v| {
        adjust(&get(v, "primaryColor"), &[('h', -90.0), ('l', -40.0)])
    });
    set_if(vars, "pie12", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 120.0), ('l', -30.0)])
    });
    set_if(vars, "pieTitleTextSize", |_| "25px".to_owned());
    set_if(vars, "pieTitleTextColor", |v| get(v, "taskTextDarkColor"));
    set_if(vars, "pieSectionTextSize", |_| "17px".to_owned());
    set_if(vars, "pieSectionTextColor", |v| get(v, "textColor"));
    set_if(vars, "pieLegendTextSize", |_| "17px".to_owned());
    set_if(vars, "pieLegendTextColor", |v| get(v, "taskTextDarkColor"));
    set_if(vars, "pieStrokeColor", |_| "black".to_owned());
    set_if(vars, "pieStrokeWidth", |_| "2px".to_owned());
    set_if(vars, "pieOuterStrokeWidth", |_| "2px".to_owned());
    set_if(vars, "pieOuterStrokeColor", |_| "black".to_owned());
    set_if(vars, "pieOpacity", |_| "0.7".to_owned());
    set_if(vars, "gitBranchLabel0", |v| {
        invert(&get(v, "labelTextColor"))
    });
    set_if(vars, "gitBranchLabel1", |v| get(v, "labelTextColor"));
    set_if(vars, "gitBranchLabel2", |v| get(v, "labelTextColor"));
    set_if(vars, "gitBranchLabel3", |v| {
        invert(&get(v, "labelTextColor"))
    });
    for i in 4..8 {
        set_if(vars, &format!("gitBranchLabel{i}"), |v| {
            get(v, "labelTextColor")
        });
    }
}

fn ctor_base(vars: &mut Vars) {
    set(vars, "radius", "5");
    set(vars, "background", "#f4f4f4");
    set(vars, "primaryColor", "#fff4dd");
    set(vars, "noteBkgColor", "#fff5ad");
    set(vars, "noteTextColor", "#333");
    vars.insert("strokeWidth".to_owned(), Value::from(1));
    set(vars, "fontFamily", FONT_FAMILY);
    set(vars, "fontSize", "16px");
    set_bool(vars, "useGradient", true);
    set(
        vars,
        "dropShadow",
        "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
    );
}

fn update_base(vars: &mut Vars) {
    let dm = dark_mode(vars);
    set_if(vars, "primaryTextColor", |_| {
        (if dm { "#eee" } else { "#333" }).to_owned()
    });
    set_if(vars, "secondaryColor", |v| {
        adjust(&get(v, "primaryColor"), &[('h', -120.0)])
    });
    set_if(vars, "tertiaryColor", |v| {
        adjust(&get(v, "primaryColor"), &[('h', 180.0), ('l', 5.0)])
    });
    set_if(vars, "primaryBorderColor", |v| {
        mk_border(&get(v, "primaryColor"), dm)
    });
    set_if(vars, "secondaryBorderColor", |v| {
        mk_border(&get(v, "secondaryColor"), dm)
    });
    set_if(vars, "tertiaryBorderColor", |v| {
        mk_border(&get(v, "tertiaryColor"), dm)
    });
    set_if(vars, "noteBorderColor", |v| {
        mk_border(&get(v, "noteBkgColor"), dm)
    });
    set_if(vars, "noteBkgColor", |_| "#fff5ad".to_owned());
    set_if(vars, "secondaryTextColor", |v| {
        invert(&get(v, "secondaryColor"))
    });
    set_if(vars, "tertiaryTextColor", |v| {
        invert(&get(v, "tertiaryColor"))
    });
    set_if(vars, "lineColor", |v| invert(&get(v, "background")));
    set_if(vars, "arrowheadColor", |v| invert(&get(v, "background")));
    set_if(vars, "textColor", |v| get(v, "primaryTextColor"));
    set_if(vars, "border2", |v| get(v, "tertiaryBorderColor"));
    set_if(vars, "nodeBkg", |v| get(v, "primaryColor"));
    set_if(vars, "mainBkg", |v| get(v, "primaryColor"));
    set_if(vars, "nodeBorder", |v| get(v, "primaryBorderColor"));
    set_if(vars, "clusterBkg", |v| get(v, "tertiaryColor"));
    set_if(vars, "clusterBorder", |v| get(v, "tertiaryBorderColor"));
    set_if(vars, "defaultLinkColor", |v| get(v, "lineColor"));
    set_if(vars, "titleColor", |v| get(v, "tertiaryTextColor"));
    set_if(vars, "edgeLabelBackground", |v| {
        if dm {
            darken(&get(v, "secondaryColor"), 30.0)
        } else {
            get(v, "secondaryColor")
        }
    });
    set_if(vars, "nodeTextColor", |v| get(v, "primaryTextColor"));
    set_if(vars, "actorBorder", |v| get(v, "primaryBorderColor"));
    set_if(vars, "actorBkg", |v| get(v, "mainBkg"));
    set_if(vars, "actorTextColor", |v| get(v, "primaryTextColor"));
    set_if(vars, "actorLineColor", |v| get(v, "actorBorder"));
    set_if(vars, "labelBoxBkgColor", |v| get(v, "actorBkg"));
    set_if(vars, "signalColor", |v| get(v, "textColor"));
    set_if(vars, "signalTextColor", |v| get(v, "textColor"));
    set_if(vars, "labelBoxBorderColor", |v| get(v, "actorBorder"));
    set_if(vars, "labelTextColor", |v| get(v, "actorTextColor"));
    set_if(vars, "loopTextColor", |v| get(v, "actorTextColor"));
    set_if(vars, "activationBorderColor", |v| {
        darken(&get(v, "secondaryColor"), 10.0)
    });
    set_if(vars, "activationBkgColor", |v| get(v, "secondaryColor"));
    set_if(vars, "sequenceNumberColor", |v| {
        invert(&get(v, "lineColor"))
    });
    set_if(vars, "transitionColor", |v| get(v, "lineColor"));
    set_if(vars, "transitionLabelColor", |v| get(v, "textColor"));
    set_if(vars, "stateLabelColor", |v| {
        let sb = get(v, "stateBkg");
        if sb.is_empty() {
            get(v, "primaryTextColor")
        } else {
            sb
        }
    });
    set_if(vars, "stateBkg", |v| get(v, "mainBkg"));
    set_if(vars, "labelBackgroundColor", |v| get(v, "stateBkg"));
    set_if(vars, "compositeBackground", |v| {
        let bg = get(v, "background");
        if bg.is_empty() {
            get(v, "tertiaryColor")
        } else {
            bg
        }
    });
    set_if(vars, "altBackground", |v| get(v, "tertiaryColor"));
    set_if(vars, "compositeTitleBackground", |v| get(v, "mainBkg"));
    set_if(vars, "compositeBorder", |v| get(v, "nodeBorder"));
    set(vars, "innerEndBackground", get(vars, "nodeBorder"));
    set(vars, "specialStateColor", get(vars, "lineColor"));
    set_if(vars, "errorBkgColor", |v| get(v, "tertiaryColor"));
    set_if(vars, "errorTextColor", |v| get(v, "tertiaryTextColor"));
    set(vars, "gradientStart", get(vars, "primaryBorderColor"));
    set(vars, "gradientStop", get(vars, "secondaryBorderColor"));
}

fn ctor_dark(vars: &mut Vars) {
    set(vars, "radius", "5");
    set(vars, "background", "#333");
    set(vars, "primaryColor", "#1f2020");
    let p = get(vars, "primaryColor");
    set(vars, "secondaryColor", lighten(&p, 16.0));
    set(vars, "tertiaryColor", adjust(&p, &[('h', -160.0)]));
    set(vars, "primaryBorderColor", invert(&get(vars, "background")));
    let dm = dark_mode(vars);
    set(
        vars,
        "secondaryBorderColor",
        mk_border(&get(vars, "secondaryColor"), dm),
    );
    set(
        vars,
        "tertiaryBorderColor",
        mk_border(&get(vars, "tertiaryColor"), dm),
    );
    set(vars, "primaryTextColor", invert(&p));
    set(
        vars,
        "secondaryTextColor",
        invert(&get(vars, "secondaryColor")),
    );
    set(
        vars,
        "tertiaryTextColor",
        invert(&get(vars, "tertiaryColor")),
    );
    set(vars, "lineColor", invert(&get(vars, "background")));
    set(vars, "textColor", invert(&get(vars, "background")));
    set(vars, "mainBkg", "#1f2020");
    set(vars, "mainContrastColor", "lightgrey");
    set(vars, "border1", "#ccc");
    set(vars, "border2", rgba(255.0, 255.0, 255.0, 0.25));
    set(vars, "fontFamily", FONT_FAMILY);
    set(vars, "fontSize", "16px");
    set(vars, "labelBackground", "#181818");
    set(vars, "textColor", "#ccc");
    set(vars, "titleColor", "#F9FFFE");
    vars.insert("strokeWidth".to_owned(), Value::from(1));
    set(vars, "errorBkgColor", "#a44141");
    set(vars, "errorTextColor", "#ddd");
    set_bool(vars, "useGradient", true);
    set(vars, "gradientStart", get(vars, "primaryBorderColor"));
    set(vars, "gradientStop", get(vars, "secondaryBorderColor"));
    set(
        vars,
        "dropShadow",
        "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
    );
}

fn update_dark(vars: &mut Vars) {
    set(vars, "secondBkg", lighten(&get(vars, "mainBkg"), 16.0));
    set(vars, "lineColor", get(vars, "mainContrastColor"));
    set(vars, "arrowheadColor", get(vars, "mainContrastColor"));
    set(vars, "nodeBkg", get(vars, "mainBkg"));
    set(vars, "nodeBorder", get(vars, "border1"));
    set(vars, "clusterBkg", get(vars, "secondBkg"));
    set(vars, "clusterBorder", get(vars, "border2"));
    set(vars, "defaultLinkColor", get(vars, "lineColor"));
    set(
        vars,
        "edgeLabelBackground",
        lighten(&get(vars, "labelBackground"), 25.0),
    );
    set(vars, "noteBorderColor", get(vars, "secondaryBorderColor"));
    set(vars, "noteBkgColor", get(vars, "secondBkg"));
    set(vars, "noteTextColor", get(vars, "secondaryTextColor"));
    set(vars, "actorBorder", get(vars, "border1"));
    set(vars, "actorBkg", get(vars, "mainBkg"));
    set(vars, "actorTextColor", get(vars, "mainContrastColor"));
    set(vars, "actorLineColor", get(vars, "actorBorder"));
    set(vars, "signalColor", get(vars, "mainContrastColor"));
    set(vars, "signalTextColor", get(vars, "mainContrastColor"));
    set(vars, "labelBoxBkgColor", get(vars, "actorBkg"));
    set(vars, "labelBoxBorderColor", get(vars, "actorBorder"));
    set(vars, "labelTextColor", get(vars, "mainContrastColor"));
    set(vars, "loopTextColor", get(vars, "mainContrastColor"));
    set(vars, "activationBorderColor", get(vars, "border1"));
    set(vars, "activationBkgColor", get(vars, "secondBkg"));
    set_if(vars, "transitionColor", |v| get(v, "lineColor"));
    set_if(vars, "transitionLabelColor", |v| get(v, "textColor"));
    set_if(vars, "stateLabelColor", |v| {
        let sb = get(v, "stateBkg");
        if sb.is_empty() {
            get(v, "primaryTextColor")
        } else {
            sb
        }
    });
    set_if(vars, "stateBkg", |v| get(v, "mainBkg"));
    set_if(vars, "labelBackgroundColor", |v| get(v, "stateBkg"));
    set_if(vars, "compositeBackground", |v| {
        let bg = get(v, "background");
        if bg.is_empty() {
            get(v, "tertiaryColor")
        } else {
            bg
        }
    });
    set_if(vars, "altBackground", |_| "#555".to_owned());
    set_if(vars, "compositeTitleBackground", |v| get(v, "mainBkg"));
    set_if(vars, "compositeBorder", |v| get(v, "nodeBorder"));
    set(vars, "innerEndBackground", get(vars, "primaryBorderColor"));
    set(vars, "specialStateColor", "#f4f4f4");
    set_if(vars, "errorBkgColor", |v| get(v, "tertiaryColor"));
    set_if(vars, "errorTextColor", |v| get(v, "tertiaryTextColor"));
    set_if(vars, "nodeBorder", |_| "#999".to_owned());
}

fn ctor_forest(vars: &mut Vars) {
    set(vars, "radius", "5");
    set(vars, "background", "#f4f4f4");
    set(vars, "primaryColor", "#cde498");
    set(vars, "secondaryColor", "#cdffb2");
    set(vars, "background", "white");
    set(vars, "mainBkg", "#cde498");
    set(vars, "secondBkg", "#cdffb2");
    set(vars, "lineColor", "green");
    set(vars, "border1", "#13540c");
    set(vars, "border2", "#6eaa49");
    set(vars, "arrowheadColor", "green");
    set(vars, "fontFamily", FONT_FAMILY);
    set(vars, "fontSize", "16px");
    set(vars, "tertiaryColor", lighten("#cde498", 10.0));
    let dm = dark_mode(vars);
    set(
        vars,
        "primaryBorderColor",
        mk_border(&get(vars, "primaryColor"), dm),
    );
    set(
        vars,
        "secondaryBorderColor",
        mk_border(&get(vars, "secondaryColor"), dm),
    );
    set(
        vars,
        "tertiaryBorderColor",
        mk_border(&get(vars, "tertiaryColor"), dm),
    );
    set(vars, "primaryTextColor", invert(&get(vars, "primaryColor")));
    set(
        vars,
        "secondaryTextColor",
        invert(&get(vars, "secondaryColor")),
    );
    set(
        vars,
        "tertiaryTextColor",
        invert(&get(vars, "primaryColor")),
    );
    set(vars, "lineColor", invert(&get(vars, "background")));
    set(vars, "textColor", invert(&get(vars, "background")));
    set(vars, "titleColor", "#333");
    set(vars, "edgeLabelBackground", "#e8e8e8");
    set(vars, "noteBkgColor", "#fff5ad");
    set(vars, "noteBorderColor", get(vars, "border2"));
    set(vars, "noteTextColor", "black");
    set(vars, "actorBorder", get(vars, "border1"));
    set(vars, "actorBkg", get(vars, "mainBkg"));
    set(vars, "labelBoxBkgColor", get(vars, "actorBkg"));
    set(vars, "signalColor", get(vars, "textColor"));
    set(vars, "signalTextColor", get(vars, "textColor"));
    set(vars, "labelBoxBorderColor", get(vars, "actorBorder"));
    set(vars, "labelTextColor", get(vars, "actorTextColor"));
    set(vars, "loopTextColor", get(vars, "actorTextColor"));
    set(vars, "actorLineColor", get(vars, "actorBorder"));
    set_if(vars, "transitionColor", |v| get(v, "lineColor"));
    set_if(vars, "transitionLabelColor", |v| get(v, "textColor"));
    set_if(vars, "stateLabelColor", |v| {
        let sb = get(v, "stateBkg");
        if sb.is_empty() {
            get(v, "primaryTextColor")
        } else {
            sb
        }
    });
    set_if(vars, "stateBkg", |v| get(v, "mainBkg"));
    set_if(vars, "labelBackgroundColor", |v| get(v, "stateBkg"));
    set_if(vars, "compositeBackground", |v| {
        let bg = get(v, "background");
        if bg.is_empty() {
            get(v, "tertiaryColor")
        } else {
            bg
        }
    });
    set_if(vars, "altBackground", |_| "#f0f0f0".to_owned());
    set_if(vars, "compositeTitleBackground", |v| get(v, "mainBkg"));
    set_if(vars, "compositeBorder", |v| get(v, "nodeBorder"));
    set(vars, "innerEndBackground", get(vars, "primaryBorderColor"));
    set(vars, "specialStateColor", get(vars, "lineColor"));
    vars.insert("strokeWidth".to_owned(), Value::from(1));
    set(vars, "errorBkgColor", "#552222");
    set(vars, "errorTextColor", "#552222");
    set_bool(vars, "useGradient", true);
    set(vars, "gradientStart", get(vars, "primaryBorderColor"));
    set(vars, "gradientStop", get(vars, "secondaryBorderColor"));
    set(
        vars,
        "dropShadow",
        "drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))",
    );
}

fn update_forest(vars: &mut Vars) {
    set(vars, "nodeBkg", get(vars, "mainBkg"));
    set(vars, "nodeBorder", get(vars, "border1"));
    set(vars, "clusterBkg", get(vars, "secondBkg"));
    set(vars, "clusterBorder", get(vars, "border2"));
    set(vars, "defaultLinkColor", get(vars, "lineColor"));
    set_if(vars, "errorBkgColor", |v| get(v, "tertiaryColor"));
    set_if(vars, "errorTextColor", |v| get(v, "tertiaryTextColor"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_matches_captured_values() {
        let vars = theme_variables("default", &Vars::new());
        assert_eq!(get(&vars, "mainBkg"), "#ECECFF");
        assert_eq!(get(&vars, "nodeBorder"), "#9370DB");
        assert_eq!(get(&vars, "clusterBkg"), "#ffffde");
        assert_eq!(get(&vars, "clusterBorder"), "#aaaa33");
        assert_eq!(get(&vars, "lineColor"), "#333333");
        assert_eq!(get(&vars, "textColor"), "#333");
        assert_eq!(get(&vars, "titleColor"), "#333");
        assert_eq!(get(&vars, "edgeLabelBackground"), "rgba(232,232,232, 0.8)");
        assert_eq!(get(&vars, "tertiaryColor"), "hsl(80, 100%, 96.2745098039%)");
        assert_eq!(get(&vars, "errorBkgColor"), "#552222");
    }

    #[test]
    fn base_theme_with_font_size() {
        let mut overrides = Vars::new();
        overrides.insert("fontSize".into(), Value::String("18px".into()));
        let vars = theme_variables("base", &overrides);
        assert_eq!(get(&vars, "fontSize"), "18px");
        assert_eq!(get(&vars, "mainBkg"), "#fff4dd");
        assert_eq!(
            get(&vars, "errorBkgColor"),
            "hsl(220.5882352941, 100%, 98.3333333333%)"
        );
    }
}
