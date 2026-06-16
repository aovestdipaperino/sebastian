# sebastian 🦀🧜

A pixel-perfect Rust port of the [mermaid.js](https://mermaid.js.org)
diagram renderers (mermaid 11.15.0). Named after the crab from the
mermaid. For supported diagram types — **flowchart, stateDiagram-v2,
sequenceDiagram, classDiagram, timeline** — the output SVG is
**byte-for-byte identical** to the official `mmdc` (mermaid-cli) output.

The workspace contains two crates:

- **`sebastian`** — the rendering library (`sebastian::render::render_diagram`)
- **`seb`** — the CLI

```
cargo run -p seb -- -i diagram.mmd -o diagram.svg
```

The non-obvious Chrome/V8/mermaid behaviors this required are cataloged
in [docs/NUANCES.md](docs/NUANCES.md).

## What is ported

The full flowchart pipeline, ported line-by-line from the JS sources:

- **Parser** — `flow.jison` grammar + `flowDb` semantics: all node shapes
  (`[]`, `()`, `(())`, `((()))`, `{}`, `{{}}`, `([])`, `[[]]`, `[()]`, `>]`,
  `[//]`, `[\\]`, trapezoids), edge types (`-->`, `===`, `-.->`, `~~~`,
  `o--o`, `x--x`, `<-->`, labels, lengths), subgraphs (nested, with
  `direction`), `classDef`/`class`/`:::`/`style`/`linkStyle`.
- **Layout** — the dagre engine exactly as bundled in `dagre-d3-es` 7.0.14:
  network-simplex ranking, crossing minimization, Brandes-Köpf positioning,
  compound/cluster handling, plus mermaid's `mermaid-graphlib` cluster
  extraction. Validated by differential tests against the JS implementation
  (exact float equality).
- **Text metrics** — Chrome-accurate label measurement using the system
  Trebuchet MS font (advances + kerning, LayoutUnit rounding, 200px
  word-wrapping), required for identical node sizes.
- **SVG generation** — d3 `curveBasis` edges with marker offsets, the exact
  default-theme stylesheet, `classDef` CSS (CSSOM serialization), markers
  (including per-color clones), clusters, self-loop decomposition, rough.js
  two-path shapes (stadium, odd), foreignObject HTML labels (or SVG
  `<text>` labels when `htmlLabels: false`, see below), and Chrome
  `XMLSerializer`/`getBBox` semantics (f32 quantization, attribute ordering,
  DOMPurify trimming).

## Label rendering (`htmlLabels`)

Mermaid renders labels two ways, and this port matches both byte-for-byte:

- **`htmlLabels: true`** (the default) — labels are `foreignObject` HTML
  spans, exactly as `mmdc` emits them. Faithful, but `foreignObject`
  requires an HTML/CSS layout engine (a browser) to rasterize.
- **`htmlLabels: false`** — node, edge, and cluster labels become native
  SVG `<text>`/`<tspan>` (mermaid's `createFormattedText` path; node text is
  centered via the `.node .label text { text-anchor: middle }` rule), and
  `classDef` styling targets the shape elements (`rect`/`polygon`/`ellipse`/
  `circle`/`path`) instead of the foreignObject contents. This output
  rasterizes in pure-SVG renderers such as
  [resvg](https://github.com/linebender/resvg) that don't support
  `foreignObject`, which makes offline SVG → PNG conversion possible without
  a headless browser.

Select it with an init directive — `%%{init: {'htmlLabels': false,
'flowchart': {'htmlLabels': false}}}%%` — or the merged config.

## Fidelity

Two reference suites assert output against captured `mmdc` SVGs:

- `sebastian/tests/flowchart_rendering.rs` — 14 hand-written diagrams (directions,
  subgraphs, self-loops, styling, unicode, wrapping, parallel edges, …),
  byte-identical.
- `sebastian/tests/book_corpus.rs` — 553 real-world flowcharts harvested from `.md`
  books. **544 are byte-identical**, including 17 with `%%{init}%%`
  directives (themes, themeVariables, `htmlLabels:false`); 3 contain
  rough.js shapes (compared modulo mermaid's own random control points);
  6 differ only numerically (5 below 0.01px from Chrome's
  arc-decomposition arithmetic, 1 at ≤2px from a space-kerning quirk).
- `sebastian/tests/flowchart_nohtml_rendering.rs` — 12 of the flowchart
  diagrams re-rendered with `htmlLabels: false` (SVG `<text>` labels),
  byte-identical. Two cases (`chain`, `multibr`) are omitted: they differ by
  ≤0.07px / 1 f32 ULP because Chrome sizes SVG-text nodes from glyph ink
  extents (`getBBox`) while this port uses advance widths — invisible when
  rasterized.
- `sebastian/tests/state_corpus.rs` — 23 stateDiagram-v2 diagrams from the same
  books (20 byte-identical, 3 note diagrams modulo rough randomness).
- `sebastian/tests/sequence_corpus.rs` — 24 sequence diagrams, all byte-identical.
- `sebastian/tests/timeline_corpus.rs` — 4 timeline diagrams, all byte-identical.
- `sebastian/tests/class_corpus.rs` — 5 class diagrams, byte-identical modulo the
  rough rectangle/divider randomness mermaid itself embeds.

Reproducing the corpus required matching Chrome's text pipeline in detail:
HTML-entity decoding via innerHTML semantics, DOMPurify tag/attribute
stripping, `\n` → `<br />` conversion, the CoreText font-fallback cascade
(Lucida Grande, Arial Unicode, Helvetica for sub/superscripts, Apple Symbols
for math operators, Hiragino for box-drawing, Apple Color Emoji at 1.25em),
per-`font-size` label measurement, and Chrome's line-breaking rules (UAX #14
hyphen breaks, open-bracket breaks after non-alphanumerics, table min-content
expansion).

The one known exception: shapes drawn through rough.js (stadium `([])`, odd
`>]`) embed *random* curve parameterization in mermaid itself — two `mmdc`
runs of the same diagram differ in those bytes. The geometry is identical
(control points are collinear); rasterized comparison shows this port is
within mermaid's own run-to-run antialiasing variance (≈0.01% of channel
bytes).

## Requirements

- The Trebuchet MS font (preinstalled on macOS and Windows) — text metrics
  and therefore the entire layout depend on it.

## Development

- `cargo test` — unit tests, dagre differential fixtures, byte-exact
  rendering tests.
- `/tmp`-based reference tooling and porting details: see `PORTING_NOTES.md`.
