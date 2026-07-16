# sebastian

<p align="center">
  <img src="sebastian/resources/LOGO.png" alt="sebastian logo" width="256">
</p>

A pixel-perfect Rust port of the [mermaid.js](https://mermaid.js.org)
diagram renderers (mermaid 11.15.0). Named after Sebastian, the crab
from Disney's *The Little Mermaid* — a fitting mascot for a
mermaid renderer. For supported diagram types — **flowchart, stateDiagram-v2,
sequenceDiagram, classDiagram, timeline, pie, erDiagram, xychart-beta,
gantt, gitGraph, journey, quadrantChart, packet, radar, sankey, block, treemap,
kanban** — the output SVG is
**byte-for-byte identical** to the official `mmdc` (mermaid-cli) output.

Four more types — **mindmap, architecture, requirement, C4** — render as
**approximate** (non-byte-exact) diagrams, and **flowchart ELK layout**
(`layout: elk`) is available behind an opt-in feature; see the status table
below for exactly what each guarantees.

The workspace contains two crates:

- **`sebastian`** — the rendering library (`sebastian::render_diagram`)
- **`seb`** — the CLI

```
cargo run -p seb -- -i diagram.mmd -o diagram.svg
```

Try it in the browser (WASM, no server round-trip):
**<https://aovestdipaperino.github.io/sebastian/>**

## Install

```sh
brew install aovestdipaperino/tap/seb   # Homebrew (macOS/Linux)
cargo install seb                       # build from crates.io
cargo binstall seb                      # or grab a prebuilt binary
```

Prebuilt binaries for macOS (arm64/x86_64), Linux (x86_64/aarch64) and
Windows are attached to each [GitHub release](https://github.com/aovestdipaperino/sebastian/releases).
The library crate is [`sebastian`](https://crates.io/crates/sebastian).

Run `seb` with no arguments (or `seb --logo`) to print the sebastian logo
as true-color terminal art, rendered from `sebastian/resources/LOGO.png`
via the [`logo-art`](https://crates.io/crates/logo-art) crate.

The non-obvious Chrome/V8/mermaid behaviors this required are cataloged
in [FINDINGS.md](FINDINGS.md).

## Status

Every "done" row is verified by a corpus test that byte-diffs sebastian's
output against captured `mmdc` (mermaid 11.15.0) SVGs; the count is the
number of fixtures under `sebastian/tests/`. "Byte-exact modulo …" means
the only differences come from randomness or render-time state that
mermaid itself embeds, so no port can match those bytes.

| Diagram type | Status | Fixtures | Notes |
|---|---|---:|---|
| flowchart / graph | ✅ done | 553 + 14 | 544/553 corpus byte-identical; remainder is rough.js randomness and sub-0.01px arc noise |
| sequenceDiagram | ✅ done | 37 | blocks, activations, autonumber, boxes, actor figures |
| stateDiagram-v2 | ✅ done | 29 | 23 byte-exact, 6 modulo rough.js + random divider id |
| classDiagram | ✅ done | 9 | byte-exact modulo rough.js rectangle/divider randomness |
| gantt | ✅ done | 5 | byte-exact modulo the render-time today marker |
| timeline | ✅ done | 4 | byte-exact |
| pie | ✅ done | 4 | byte-exact |
| erDiagram | ✅ done | 3 | byte-exact modulo rough.js randomness |
| xychart-beta | ✅ done | 3 | byte-exact |
| gitGraph (`LR` / `TB` / `BT`) | ✅ done | 10 | byte-exact modulo random commit ids + 1-ulp viewBox/max-width |
| journey | ✅ done | 3 | byte-exact |
| quadrantChart | ✅ done | 3 | byte-exact |
| packet / packet-beta | ✅ done | 3 | byte-exact |
| radar / radar-beta | ✅ done | 3 | byte-exact |
| sankey / sankey-beta | ✅ done | 3 | byte-exact (labels-within-bounds; getBBox ignores text) |
| block / block-beta | ✅ done | 12 | byte-exact (columns, space, spans, composites, classDef/style, edges incl. labels) |
| treemap / treemap-beta | ✅ done | 4 | byte-exact (d3 squarify layout, sections/leaves, font-shrink labels) |
| kanban | ✅ done | 3 | byte-exact (mindmap-indent parser, section clusters + item cards, arithmetic column layout) |
| flowchart ELK layout | 🟢 in progress | 4 (elk feat) | opt-in `elk` feature routes `layout: elk` through `elkrs` end-to-end; flat graphs (all directions), self-loops, and multi-edges use real ELK layout (node placement byte-exact from exact dims, ~1/128px in full render); subgraphs fall back to dagre so their cluster boxes are correct; byte-exact edge geometry + the node-dim gap + native ELK clusters remain — scoped below |
| mindmap | 🟡 approximate | smoke | renders with a deterministic tidy-tree layout; **not byte-exact** (mermaid uses the cose-bilkent force engine) |
| architecture | 🟡 approximate | smoke | renders with a deterministic directional grid; **not byte-exact** (mermaid uses cytoscape-`fcose`, `Math.random`-seeded) |
| requirement | 🟡 approximate | smoke | reuses the unified dagre pipeline as multi-line boxes; **byte-exact closed as intractable** (box `max-width` doesn't match ground-truth Chrome `getBBox`) |
| C4 (Context/Container/Component/Dynamic/Deployment) | 🟡 approximate | smoke | deterministic row-based layout; **byte-exact closed as intractable** (same `getBBox` finding) |
| pyramid | 🔵 extension | smoke | **sebastian-only** diagram (no mermaid equivalent): pyramid chart + pyramid of components; original renderer, not a port. Behind the `mermaid-extensions` feature (on by default) |

> **Note on mindmap and architecture (approximate renderers).** Mermaid lays
> both out with force-directed engines that have no byte-exact path here:
> **mindmap** forces `layout: cose-bilkent` (deterministic, but a physics engine
> that would be an ELK-tier port), and **architecture** uses cytoscape-`fcose`,
> which is `Math.random`-seeded and *not even deterministic run-to-run*. So
> sebastian ships **approximate** renderers for these two — its own
> deterministic layouts (a left-to-right tidy tree for mindmap, a directional
> grid for architecture). They produce clean, stable output but are **not
> byte-identical to mmdc**; like the hand-drawn look, they are an explicit
> opt-out of the byte-exact guarantee, validated by structural smoke tests
> rather than the byte-diff corpus.

> **Note on requirement and C4 (approximate renderers).** These two are blocked
> on font metrics, not effort. Both size their boxes with mermaid's
> `calculateTextDimensions`, which measures Blink `getBBox()` **ink extents**
> over `sans-serif`/`Arial` (or Trebuchet), `Math.round`ed per line — and those
> integers land verbatim in the output (a requirement label's `max-width`, a C4
> shape's width). A 2026-07 investigation probed the **same** headless Chrome
> that generated the references and found the emitted values don't match its
> `getBBox` at the real `fontSize: 16` by any family-selection rule (e.g.
> `&lt;&lt;Requirement&gt;&gt;` → Chrome 211/195, reference 193 — *below even the
> sans-serif ink*; implied font size scatters 13–15px across strings). So the
> target isn't a stable, reproducible quantity and byte-exact is closed as
> intractable, not merely unimplemented. sebastian ships **approximate**
> renderers — requirement
> reuses the unified dagre pipeline (multi-line boxes + shared markers); C4 uses
> its own deterministic row-based layout with native-SVG boxes, person heads,
> and relationship arrows. Clean and stable, but **not byte-identical to mmdc**;
> validated by structural smoke tests.

## How to help

The porting loop is mechanical once you have the reference output, and it
is the same loop that got every ✅ row to byte-exact (details in
[PORTING_NOTES.md](PORTING_NOTES.md)):

1. Harvest real diagrams of the target type into a `tests/<type>_cases/`
   directory as `.mmd` files.
2. Render each with `mmdc` (mermaid 11.15.0, headless Chrome on macOS) to
   capture the reference SVGs.
3. Byte-diff sebastian's output against the reference and chase the first
   differing byte until the diff is empty.
4. Add a corpus test with an identical-count guard so regressions surface.

The highest-leverage contributions right now:

- **Finishing the `elkrs`-backed ELK layout** (`layout: elk`, the `elk`
  feature). It is already wired end-to-end with byte-exact node placement; the
  remaining pieces — closing the node-dimension 1/128 gap, byte-exact edge
  routing, and native ELK cluster layout — are scoped in the section below and
  in `TODO.md`. All three are gated on the same sub-pixel `getBBox` artifact.

(Note: byte-exact **requirement** and **C4** were investigated and closed as
intractable — their box widths don't match ground-truth Chrome `getBBox` at the
real font size, so no ink-metrics subsystem would reproduce them. See `TODO.md`.)

If a diagram type you need is missing, opening a PR with `.mmd` fixtures
and their `mmdc` references is the most useful first step even before any
Rust is written.

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

## Hand-drawn look (`look: handDrawn`)

`%%{init: {'look': 'handDrawn'}}%%` turns on an Excalidraw-style look.
Labels in **every** diagram type render in
[Excalifont](https://plus.excalidraw.com/excalifont) — Excalidraw's own
handwriting font (SIL OFL, successor to Virgil) — which is embedded in the
crate and inlined into the SVG as a `@font-face` data URI (~256 KB), so the
output looks the same on every machine with no font installed. The stack
falls back to `"Comic Sans MS", "Chalkboard SE", "Bradley Hand", cursive`.
The override is injected once at the `render_diagram` boundary, covering
both HTML and raw SVG text labels. Sketchy, double-stroked outlines are
drawn for:

- **flowchart, stateDiagram, classDiagram** — node shapes and edges (the
  shared unified pipeline);
- **sequenceDiagram** — actor/note/loop boxes and message lines;
- **timeline, journey, gantt, kanban, packet, quadrant, xychart, treemap,
  gitGraph** — their primary boxes, bars, cards and commit bullets get a
  wobbly outline over the crisp CSS-styled fill.

Types whose classic look is already rough (erDiagram, requirement) or that
are dominated by curves (pie, sankey, radar) keep their shapes and get the
handwritten font only.

Unlike the classic look, this is an **opt-in stylization, not byte-exact**.
Upstream mermaid draws hand-drawn shapes with rough.js seeded from
`Math.random`, so two `mmdc` runs of the same diagram differ. sebastian
instead uses a deterministic seeded PRNG (a port of rough.js's `mulberry32`),
so its hand-drawn output is stable run to run. Text is measured with the
Excalifont metrics themselves, so node and label boxes fit the handwritten
glyphs exactly (classic-look renders are untouched and stay byte-exact).
Pairs naturally with `htmlLabels: false` for offline rasterization — the
rasterizer registers the embedded face, so PNGs come out in Excalifont too.

Excalifont ships a single weight, so **bold** text (class titles,
timeline/journey titles, treemap section headers, markdown `**bold**`) is
emulated explicitly: bold runs are painted with a thin stroke in their own
text color, which renders identically in browsers and in resvg — instead
of relying on the browser's synthetic bold, which resvg lacks entirely.

On hosts without the real fonts (wasm without registered fonts, bare
Linux), classic-look renders apply the same principle: the embedded face
that measured the text (Cabin, or Tinos for sequence diagrams) is inlined
into the SVG and used for drawing, so output is self-consistent everywhere
instead of depending on the viewer's installed fonts.

### Sequence diagrams (sebastian extension)

Hand-drawn support for **sequence diagrams** is a sebastian-specific extension
with **no upstream equivalent** — mermaid's legacy sequence renderer
(`sequenceRenderer.ts` + `svgDraw.js`) ignores `look` and always draws crisp
shapes. When `look: handDrawn` is set, sebastian routes the sequence diagram's
actor boxes, footer boxes, note boxes, straight message lines, and loop/alt
borders through the same sketchy primitives the flowchart uses. By design it
leaves a few elements crisp: self-message bezier curves, the loop label tab,
the thin lifelines, and arrowhead markers. See
`sebastian/src/sequence/render.rs` (module docs) and
`sebastian/tests/sequence_handdrawn.rs`.

## Pyramid diagrams (sebastian extension)

`pyramid` is a **sebastian-only** diagram type with no mermaid equivalent, so
it is an original renderer (not byte-exact against anything). It lives behind
the **`mermaid-extensions`** cargo feature, which is **on by default** — build
with `--no-default-features` to restrict sebastian to faithful mermaid diagram
types only. It draws stacked
trapezoid bands as a triangle — narrow apex on top, wide base at the bottom —
one labelled band per level:

```
pyramid
  title Company Hierarchy
  CEO
  Directors
  Managers
  Staff
```

Add a `: a, b, c` component list to any level and that band becomes a **pyramid
of components** — the named boxes are laid out in a row inside the band. The two
forms mix freely:

```
pyramid
  title System Architecture
  Presentation: Web, Mobile
  Business: Auth, Orders, Billing
  Data: Postgres, Redis, Queue
```

Layout is deterministic and band colours come from the theme `cScale` palette;
it renders in pure-SVG rasterizers (native `<text>`, no `foreignObject`).
Prefixing the chart with `%%{init: {"look": "handDrawn"}}%%` sketches the
bands and component boxes with wobbly hand-drawn outlines, matching the
flowchart look:

<p align="center">
  <img src="docs/pyramid-handdrawn.png" alt="hand-drawn pyramid of components" width="640">
</p>

Validated by `sebastian/tests/pyramid_smoke.rs`.

## System charts (sebastian extension)

`system_chart` is a second **sebastian-only** diagram type behind the same
**`mermaid-extensions`** feature: boxes with typical system-component icons,
connected by labelled arrows, expressing a system architecture.

```
system_chart
  title Query pipeline
  query: chat "AI Agent Query" "What is our churn rate?"
  rt: router "Router" "(Classify)"
  okf: wiki "OKF" "(Wiki)"
  rag: db "RAG" "(Vector DB)"
  ai: llm "LLM" "(Synthesize)"
  query --> rt
  rt --> okf : Canonical?
  rt --> rag : Exploratory?
  okf --> ai
  rag --> ai
```

### Writing a system chart

A chart is a sequence of lines; leading whitespace is ignored and lines
starting with `%%` or `#` are comments.

1. **Header** — the first non-comment line must be `system_chart`.
2. **Title** (optional) — `title My system` draws a centred heading.
   A `legend` line (optional, off by default) draws a key of the connection
   types the chart actually uses, placed in a free corner (the canvas grows
   below the chart when every corner is occupied).
3. **Nodes** — one per line, declared before any edge that uses them:

   ```
   id: symbol "Title" "Optional subtitle"
   ```

   `id` is the name edges refer to, `symbol` picks the icon and colour
   scheme (table below), the quoted `"Title"` is required, and a second
   quoted string adds a smaller grey subtitle.
4. **Edges** — `from OP to`, optionally followed by `: label`. The operator
   encodes the connection type in the line style:

   | operator | meaning | style |
   |----------|---------|-------|
   | `a --> b` | synchronous call / request | solid arrow |
   | `a ..> b` | event trigger / async notification | dashed arrow |
   | `a ==> b` | message via queue or bus | thick arrow, envelope at midpoint |
   | `a --- b` | undirected association | thin line, no arrowhead |

   Every operator accepts a label: `rt ==> jobs : task`. Edges take the
   accent colour of their **source** node. Referencing an undeclared node is
   a parse error.

### Symbols

Each symbol has its own accent colour and icon:

| symbol | use for | symbol | use for |
|--------|---------|--------|---------|
| `user` | end user | `api` | API gateway / endpoint |
| `users` | user group | `fn` | serverless function |
| `chat` | message / query | `stream` | event stream (Kafka-style) |
| `queue` | message queue | `scheduler` | cron / timed jobs |
| `folder` | file storage | `browser` | web frontend |
| `db` | database | `mobile` | mobile app |
| `wiki` | knowledge base | `metrics` | monitoring / observability |
| `router` | router / load balancer | `mail` | email / notification |
| `llm` | LLM / AI model | `bucket` | object storage |
| `doc` | document | `key` | secrets store |
| `cloud` | cloud service | `robot` | agent / bot / automation |
| `service` | service / worker | `search` | search index |
| `lock` | auth / security boundary | `box` | generic component |
| `server` | server / host | `cache` | cache / fast path |
| `file` | single file | `files` | set of files |

`box` is also the fallback for unknown symbol names. One chart exercising
every symbol, all four connection types, and the `legend` option, in the
hand-drawn look (`SUPPORTED-SYMBOLS-HANDDRAWN.png`; `SUPPORTED-SYMBOLS.png`
is the same chart in the classic look):

<p align="center">
  <img src="SUPPORTED-SYMBOLS-HANDDRAWN.png" alt="all system_chart symbols and connection types, hand-drawn" width="900">
</p>

### Layout and looks

Nodes are ranked top-to-bottom by longest path from the sources; within a
rank they keep declaration order, so declare nodes left-to-right in the order
you want them to appear. Layout is deterministic and renders in pure-SVG
rasterizers (native `<text>`, no `foreignObject`). Prefixing the chart with
`%%{init: {"look": "handDrawn"}}%%` switches to the hand-drawn look: sketchy
double-stroked boxes, wobbly edges, and the handwritten font (arrowheads,
envelope glyphs, and icons stay crisp by design). The example above renders
as:

<p align="center">
  <img src="docs/system-chart-handdrawn.png" alt="hand-drawn system chart" width="420">
</p>

Validated by `sebastian/tests/system_chart_smoke.rs`.

## Rasterization (PNG) — `raster` feature

The renderers return **SVG** by default. Enable the `raster` feature to also get
**PNG** via the `render::raster` module (pulls in resvg; off by default so
SVG-only consumers stay light):

- `render_png(source, id)` — mermaid source straight to PNG bytes.
- `rasterize_svg(svg, &RasterOptions)` — rasterize any SVG, with an optional
  background, an extra blank footer band, and an overlay SVG composited on top
  (e.g. a caller's watermark).
- `measure_svg(svg)` — the rendered pixel size, for sizing an overlay first.

Fonts are selected by `RasterOptions::fonts`:

- `FontSource::Embedded` (default) bundles **Cabin** (SIL OFL 1.1) and points
  every generic family at it, so the standard `trebuchet ms, …, sans-serif`
  stack falls through to Cabin. No installed fonts required; output is
  deterministic across machines.
- `FontSource::System` loads the system fonts and leaves family resolution to
  usvg, so `trebuchet ms` resolves to the installed face — **use this for
  pixel-perfect raster comparison against mermaid-cli**, whose Chrome renders the
  same stack.

```bash
cargo build --features raster
```

## Performance

The reason this project exists in one number: `mmdc` renders by launching a
headless Chrome; sebastian is a native binary. Measured with
[hyperfine](https://github.com/sharkdp/hyperfine) on an Apple M5 Max
(macOS 26.5), rendering the same flowchart to byte-identical SVG:

| | mean per diagram | vs mmdc |
|---|---:|---:|
| `seb` 0.3.0 | 22.5 ms ± 1.2 ms | **~30× faster** |
| `mmdc` (mermaid-cli 11.12, mermaid 11.15) | 670.1 ms ± 3.2 ms | 1× |

Most of seb's 22 ms is process startup (font loading); the render itself is
single-digit milliseconds, so batch use through the library API is faster
still. Rendering the full 553-diagram book corpus takes seb **14.3 s**
(one process per diagram, ~26 ms each); at mmdc's per-diagram cost the same
corpus takes about **6 minutes**.

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
- `sebastian/tests/state_corpus.rs` — 29 stateDiagram-v2 diagrams (23
  byte-identical; 6 compared modulo mermaid's own rough-path randomness
  and its random divider `generateId()` token).
- `sebastian/tests/sequence_corpus.rs` — 37 sequence diagrams (blocks,
  activations, autonumber, boxes, actor figures), all byte-identical.
- `sebastian/tests/timeline_corpus.rs` — 4 timeline diagrams, all byte-identical.
- `sebastian/tests/pie_corpus.rs`, `sebastian/tests/er_corpus.rs`,
  `sebastian/tests/xychart_corpus.rs`, `sebastian/tests/gantt_corpus.rs` —
  pie, ER, xychart-beta, and gantt fixtures (byte-identical; ER modulo
  mermaid's rough randomness, gantt modulo the render-time today marker).
- `sebastian/tests/class_corpus.rs` — 9 class diagrams (generics, notes,
  namespaces, lollipop interfaces), byte-identical modulo the rough
  rectangle/divider randomness mermaid itself embeds.
- `sebastian/tests/gitgraph_corpus.rs` — 10 gitGraphs (`LR`/`TB`/`BT`, incl.
  commit labels + tags), byte-identical modulo the `Math.random()`-seeded
  auto-generated commit ids and a single-f32-ulp viewBox/max-width difference
  in Blink's rotated-rect bbox mapping.
- `sebastian/tests/journey_corpus.rs` — 2 user-journey diagrams, all
  byte-identical.
- `sebastian/tests/quadrant_corpus.rs`, `sebastian/tests/packet_corpus.rs`,
  `sebastian/tests/radar_corpus.rs`, `sebastian/tests/sankey_corpus.rs`,
  `sebastian/tests/block_corpus.rs`, `sebastian/tests/treemap_corpus.rs`,
  `sebastian/tests/kanban_corpus.rs` — quadrantChart, packet-beta, radar-beta,
  sankey-beta, block-beta, treemap-beta, and kanban fixtures, all byte-identical.
- `sebastian/tests/approximate_smoke.rs` — structural smoke tests for the
  **approximate** renderers (mindmap, architecture, requirement, C4) and the
  ELK dagre-fallback path: valid SVG with the expected elements, not byte-diff.
- `sebastian/tests/elk_layout.rs` (needs `--features elk`) — the `elk` backend:
  byte-exact node placement vs mermaid's elkjs, near-exact full render, subgraph
  fallback, and direction/self-loop coverage.

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

- For **byte-exact** output: the Trebuchet MS font (preinstalled on macOS and
  Windows) — text metrics and therefore the entire layout depend on it.
  Sequence diagrams additionally use Times New Roman.
- Without those fonts (bare Linux, wasm), sebastian falls back to embedded
  SIL-OFL faces — Cabin for Trebuchet MS, Tinos (Times-metric-compatible)
  for Times New Roman. Output is well-proportioned but not byte-exact.
  Hosts can also supply real font bytes at runtime via
  `sebastian::text::register_font(file_name, bytes)`, which takes precedence
  over system fonts.
- On Windows (and wasm), gantt calendar arithmetic runs in UTC rather than
  the system timezone — the C runtime there lacks `localtime_r`/`mktime`.

## WebAssembly

`sebastian` compiles for `wasm32-unknown-unknown`; the `sebastian-wasm`
workspace crate wraps it with wasm-bindgen (`render`, `detect_diagram_type`,
`register_font`) and ships a browser demo — live at
<https://aovestdipaperino.github.io/sebastian/>, deployed from `main` by
`.github/workflows/pages.yml`. Build locally with
`wasm-pack build sebastian-wasm --target web` and see
[`sebastian-wasm/README.md`](sebastian-wasm/README.md). wasm-only caveats:
gantt date math runs in UTC, and transcendentals use `libm` instead of the
correctly-rounded `core-math`, so final-ULP coordinate differences vs mmdc
are possible.

## Flowchart ELK layout (`elk` feature, in progress)

`%%{init: {"layout": "elk"}}%%` (or `flowchart.defaultRenderer: "elk"`)
routes layout through the Java ELK *layered* engine. Rather than porting
that engine, sebastian reuses the native-Rust [`elkrs`](https://crates.io/crates/elkrs)
crate — a 2026-07 spike proved it is **byte-identical to mermaid's elkjs
0.9.x on acyclic layered graphs** (it diverges only on cyclic graphs, where
0.9→0.11 cycle-breaking changed). Enable it with the opt-in `elk` cargo
feature (keeps `elkrs`'s dependencies out of the default build):

```bash
cargo build --features elk       # library
cargo run -p seb --features sebastian/elk -- -i x.mmd -o x.svg
```

What works today (`sebastian/tests/elk_layout.rs`):

- **Node placement is byte-exact** given exact input dimensions — the exact
  ELK-JSON mermaid feeds elkjs, run through `elkrs`, matches coordinate for
  coordinate. In a full render it lands within ~1/128px.
- Flat graphs in **all directions** (TB/BT/LR/RL), **self-loops**, and
  **multi-edges** use real ELK layout; straight edges match mermaid to ~1/128.
- Flowcharts **with subgraphs** fall back to dagre for the whole render, so
  their cluster boxes are correct (native ELK cluster layout is not ported).

Remaining work (all scoped in `TODO.md`), none of it byte-exact yet because
it is gated on one sub-pixel artifact — mermaid measures ELK node widths in a
throwaway CSS-less container, so its `getBBox` width differs from the (byte-exact)
dagre width by a node-dependent 0-or-1/128:

1. the node-dimension gap (needs modeling mermaid's ELK-context `getBBox`),
2. byte-exact edge routing (the `@mermaid-js/layout-elk` geometry glue is
   mapped in `TODO.md`; edge endpoints inherit the node-dim gap), and
3. native ELK cluster layout (a nested `INCLUDE_CHILDREN` graph).

## Development

- `cargo test` — unit tests, dagre differential fixtures, byte-exact
  rendering tests.
- `cargo test --features elk` — additionally runs the ELK backend tests
  (`elk_layout.rs`).
- `/tmp`-based reference tooling and porting details: see `PORTING_NOTES.md`.
- The gantt corpus references were generated in Pacific time — run
  `TZ=America/Los_Angeles cargo test -p sebastian --test gantt_corpus`
  (CI pins this TZ; in other zones the corpus diffs).
- CI (GitHub Actions): fmt/clippy/test on macOS (the corpora need the real
  fonts), a wasm build check, and automatic publishing of `sebastian` then
  `seb` to crates.io on `v*` tags matching the workspace version.
- Rendering nuances and environment/tooling gotchas are both collected in
  [`FINDINGS.md`](FINDINGS.md).
