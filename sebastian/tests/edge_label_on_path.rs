//! `edgeLabelOnPath` (sebastian extension): edge labels ride the arc-length
//! midpoint of the drawn path instead of dagre's label-dummy coordinate.
//!
//! Off by default, because mermaid places labels at `edge.x`/`edge.y` and the
//! byte-exact corpora depend on that. These tests pin both halves: default
//! output is unchanged, and the opt-in actually moves labels onto their edges.

use sebastian::render_diagram;

/// A fan of curved edges out of one state, which is where dagre's sideways
/// label offset drifts furthest from the line it names.
const FAN: &str = "\
stateDiagram-v2
    [*] --> Closed
    Closed --> Listening : listen()
    Closed --> Connected : connect()
    Listening --> Established : accept()
    Listening --> Closed : close()
    Connected --> Established : handshake()
    Connected --> Closed : close()
    Established --> Closed : close()
";

fn with_directive(src: &str, directive: &str) -> String {
    format!("{directive}\n{src}")
}

/// Every `<g class="edgeLabel" transform="translate(x, y)">` position, in
/// document order. The inner `<g class="label">` carries the centering
/// offset and is deliberately not matched here.
fn label_positions(svg: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<g class=\"edgeLabel\" transform=\"translate(") {
        rest = &rest[i + "<g class=\"edgeLabel\" transform=\"translate(".len()..];
        let end = rest.find(')').expect("closing paren");
        let (a, b) = rest[..end].split_once(", ").expect("x, y");
        out.push((
            a.parse().expect("x is a number"),
            b.parse().expect("y is a number"),
        ));
        rest = &rest[end..];
    }
    out
}

/// The `d` of every edge path (node shapes and markers carry no `data-edge`).
fn edge_path_ds(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<path d=\"") {
        rest = &rest[i + "<path d=\"".len()..];
        let end = rest.find('"').expect("closing quote");
        let d = &rest[..end];
        let tail = &rest[end..];
        if let Some(gt) = tail.find('>')
            && tail[..gt].contains("data-edge=\"true\"")
        {
            out.push(d.to_owned());
        }
        rest = tail;
    }
    out
}

/// Samples the curve an SVG path `d` actually draws. Sebastian emits these
/// edges as `M x,y` followed by `L` and cubic `C` segments, so the control
/// points are *not* on the curve and cannot be used as a proxy for it.
fn sample_path(d: &str) -> Vec<(f64, f64)> {
    const STEPS: usize = 400;
    let mut samples = Vec::new();
    let mut cursor = (0.0, 0.0);
    let mut rest = d.trim();

    let take_nums = |s: &str, n: usize| -> (Vec<f64>, usize) {
        let bytes = s.as_bytes();
        let mut nums = Vec::with_capacity(n);
        let mut i = 0;
        while nums.len() < n {
            while i < bytes.len()
                && !(bytes[i].is_ascii_digit() || bytes[i] == b'-' || bytes[i] == b'.')
            {
                i += 1;
            }
            let start = i;
            if i < bytes.len() && bytes[i] == b'-' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            nums.push(s[start..i].parse::<f64>().expect("number"));
        }
        (nums, i)
    };

    while let Some(cmd) = rest.chars().next() {
        match cmd {
            'M' | 'L' => {
                let (n, used) = take_nums(&rest[1..], 2);
                cursor = (n[0], n[1]);
                samples.push(cursor);
                rest = rest[1 + used..].trim_start();
            }
            'C' => {
                let (n, used) = take_nums(&rest[1..], 6);
                let (p0, p1, p2, p3) = (cursor, (n[0], n[1]), (n[2], n[3]), (n[4], n[5]));
                for step in 1..=STEPS {
                    let t = step as f64 / STEPS as f64;
                    let u = 1.0 - t;
                    let bez = |a: f64, b: f64, c: f64, dd: f64| {
                        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * dd
                    };
                    samples.push((bez(p0.0, p1.0, p2.0, p3.0), bez(p0.1, p1.1, p2.1, p3.1)));
                }
                cursor = p3;
                rest = rest[1 + used..].trim_start();
            }
            _ => rest = &rest[1..],
        }
    }
    samples
}

fn min_distance_to_any_path(pos: (f64, f64), paths: &[String]) -> f64 {
    paths
        .iter()
        .flat_map(|d| sample_path(d))
        .map(|(x, y)| ((x - pos.0).powi(2) + (y - pos.1).powi(2)).sqrt())
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn default_is_unchanged_by_the_extension() {
    let plain = render_diagram(FAN, "my-svg").expect("renders");
    let explicit_off = render_diagram(
        &with_directive(FAN, "%%{init: {'edgeLabelOnPath': false}}%%"),
        "my-svg",
    )
    .expect("renders");

    // The directive is parsed, so the only textual difference is the source
    // echoed nowhere: output must be identical.
    assert_eq!(
        label_positions(&plain),
        label_positions(&explicit_off),
        "explicit false must match the default"
    );

    // Pin mermaid's own placement for this diagram (verified against
    // mermaid-cli 11.15.0): labels sit on the label rank, offset sideways.
    let positions = label_positions(&plain);
    assert_eq!(positions.len(), 7, "one label per labelled edge");
    assert!(
        (positions[0].0 - 33.664_062_5).abs() < 1e-6 && (positions[0].1 - 149.0).abs() < 1e-6,
        "listen() keeps dagre's label-dummy coordinate, got {:?}",
        positions[0]
    );
}

#[test]
fn opt_in_moves_labels_onto_their_edges() {
    let off = render_diagram(FAN, "my-svg").expect("renders");
    let on = render_diagram(
        &with_directive(FAN, "%%{init: {'edgeLabelOnPath': true}}%%"),
        "my-svg",
    )
    .expect("renders");

    let paths = edge_path_ds(&on);
    assert!(!paths.is_empty(), "expected edge paths");

    let off_positions = label_positions(&off);
    let on_positions = label_positions(&on);
    assert_eq!(off_positions.len(), on_positions.len());

    let (mut moved, mut worst_before, mut worst_after) = (0, 0.0f64, 0.0f64);
    let (mut total_before, mut total_after) = (0.0, 0.0);
    for (before, after) in off_positions.iter().zip(&on_positions) {
        let d_before = min_distance_to_any_path(*before, &paths);
        let d_after = min_distance_to_any_path(*after, &paths);
        worst_before = worst_before.max(d_before);
        worst_after = worst_after.max(d_after);
        total_before += d_before;
        total_after += d_after;
        if (before.0 - after.0).abs() > 1e-6 || (before.1 - after.1).abs() > 1e-6 {
            moved += 1;
        }
    }
    assert!(
        moved >= 5,
        "expected the curved edges' labels to move, only {moved} did"
    );

    // Mermaid's placement strands the worst label ~17px off the edge it names.
    // Riding the path must pull every label near its line. The residual is the
    // basis spline: `calc_label_position` walks the dagre polyline, and the
    // drawn curve only passes through that polyline's endpoints.
    assert!(
        worst_before > 15.0,
        "expected the default to strand a label, worst was {worst_before}"
    );
    assert!(
        worst_after < 8.0,
        "every label should sit near an edge, worst was {worst_after}"
    );
    assert!(
        total_after < total_before / 2.0,
        "total drift should more than halve: {total_before} -> {total_after}"
    );

    // A straight vertical edge already had its label at the visual middle, so
    // the extension must leave it alone.
    assert!(
        on_positions.contains(&(221.113_281_25, 263.0)),
        "the straight Connected -> Established label should not move, got {on_positions:?}"
    );
}
