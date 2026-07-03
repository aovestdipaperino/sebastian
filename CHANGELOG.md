# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.1] — 2026-07-03

### Changed
- Trimmed the published `sebastian` tarball ~90% (16.4 MiB → 1.2 MiB) by
  excluding the test corpora and logo (`exclude = ["tests/**",
  "resources/**"]`); fonts and the theme CSS are still bundled.

## [0.1.0] — 2026-07-03

First crates.io release: `sebastian` (library) and `seb` (CLI).

## [Unreleased]

### Added
- **radar / radar-beta** — radar (spider) charts, byte-exact vs mermaid-cli
  11.15.0. Ports the langium radar grammar (axes, curves with plain or
  axis-referenced entries, and `showLegend`/`ticks`/`max`/`min`/`graticule`
  options), `db.ts`, and the self-contained polar `renderer.ts`
  (circle/polygon graticule, axes, Catmull-Rom `closedRoundCurve`, legend,
  title). No text measurement; all coordinates come from `Math.cos`/`Math.sin`
  via the `core-math` crate (V8-matching) and constants. Three corpus cases.
- **packet / packet-beta** — bit-field packet diagrams, byte-exact vs
  mermaid-cli 11.15.0. Ports the langium packet grammar (`start(-end)?` and
  `+bits` block forms), `db.ts` (`populate` / `getNextFittingBlock` row
  wrapping at `bitsPerRow`), and `renderer.ts`. Layout is pure arithmetic
  (no text measurement). Three corpus cases: TCP header (256-bit, 8-row
  wrap), `+bits` syntax, and a no-title diagram.
- More corpus fixtures for existing types (pie, journey, quadrantChart,
  xychart, erDiagram, gitGraph) — all byte-exact (er masked for rough.js
  randomness; gitGraph masked for the known 1e-3 viewBox rounding).

### Changed
- Renamed the project to **sebastian** (the crab from the mermaid) and
  split it into a workspace: the `sebastian` library crate and the `seb`
  CLI crate.

### Added
- **`seb` CLI logo banner** — the embedded `sebastian/resources/LOGO.png`
  is rendered as true-color terminal ANSI art (via the `logo-art` crate)
  on the no-args usage banner and the explicit `seb --logo` flag.
- **gitGraph** (`LR` orientation) — commit/branch/merge/cherry-pick parsing,
  branch lanes, commit bullets and labels, and the themed git palette.
  Byte-exact modulo the `Math.random()`-seeded auto-generated commit ids and
  a single-f32-ulp viewBox difference in Blink's rotated-rect bbox mapping.
- **journey** (user-journey) — task/section parsing, actor legend, the
  section color scale, and the smiley score faces. All fixtures byte-exact.
- **quadrantChart** — quadrant rects, external/internal borders, axis
  labels (center vs left anchor by paired-label presence), and data points
  via d3 scaleLinear interpolation. Reproduces the upstream
  operator-precedence bug that renders point fills as
  `hsl(240, 100%, NaN%)`. All fixtures byte-exact.
- **Four diagram types, byte-exact vs mermaid-cli 11.15.0:**
  - **pie** — d3 arc sectors (digits-3 path serializer), theme pie1-12
    palette, CSSOM legend styles.
  - **erDiagram** — entity attribute grids (erBox), crow's-foot markers,
    Blink-exact Times ink text measurement.
  - **xychart-beta** — chartBuilder orchestrator, band/linear axes with the
    d3 ticks algorithm and bimap semantics, bar and line plots.
  - **gantt** — dayjs-style date parsing (naive-local), d3 scaleTime
    rangeRound, d3-time tick intervals (incl. the day.every day-of-month
    anchoring), d3-axis markup; the today marker follows render time as
    upstream does.
- **Sequence diagram gap features**, byte-exact: `alt`/`opt`/`par`/`critical`/
  `break`/`rect` blocks (with `else`/`and`/`option` sections), activations
  (`+`/`-` shorthand and `activate`/`deactivate`, stacked), `autonumber`
  (sequence-number circles and start/step), `box` participant groupings, and
  `actor` stick figures. Corpus grew to 34 cases.
- **State diagram gap features**: `<<fork>>`/`<<join>>`/`<<choice>>` shapes,
  composite states (`roundedWithTitle` clusters), concurrency dividers (`--`,
  including upstream's trailing-section `generateId()` quirk), and `classDef`/
  `class` styling with generated CSS. Corpus grew to 29 cases.
- **Class diagram gap features**: `Name~T~` generics (escaped-title
  measurement quirk included), `note` / `note for` with dotted note edges,
  `namespace` clusters, and lollipop interfaces (`--()`), plus a faithful
  CSSOM merge for label div styles. Corpus grew to 9 cases.
- `docs/NUANCES.md`: the catalog of Chrome/V8/mermaid behaviors discovered
  while reaching byte-exact output.
- **Hand-drawn sequence diagrams (sebastian extension, no upstream equivalent).**
  `look: handDrawn` now also stylizes sequence diagrams: actor/footer/note boxes,
  straight message lines, and loop borders render with the sketchy
  `hd_polygon`/`hd_edge_d` primitives. Mermaid's legacy sequence renderer ignores
  `look`, so this is a deliberate divergence (self-message curves, loop label
  tabs, lifelines, and arrowhead markers stay crisp by design). Crisp sequence
  output is byte-for-byte unchanged. See `tests/sequence_handdrawn.rs`.
- **`raster` feature: SVG → PNG rendering (`render::raster`).** Optional (pulls in
  resvg) so pure-SVG consumers stay light. Exposes `render_png`, `rasterize_svg`
  (with background, an extra footer band, and an overlay SVG for callers'
  watermarks), and `measure_svg`. Ships an embedded Cabin font (SIL OFL 1.1) so
  output needs no installed fonts and is deterministic across machines
  (`FontSource::Embedded`, default); `FontSource::System` loads system fonts for
  pixel-perfect raster comparison against mermaid-cli.

## [0.1.0] - 2026-06-12

### Added

#### Flowchart renderer
- Full port of the mermaid 11.15.0 flowchart pipeline: flow.jison
  lexer/parser, flowDb, dagre + dagre-d3-es 7.0.14 layout (differential-
  tested against 17 upstream fixtures), unified dagre rendering (nodes,
  edges, markers, clusters, self-loop decomposition, recursive cluster
  layouts), and Chrome-exact SVG serialization.
- Chrome text metrics: Trebuchet MS advance+kern measurement on the
  LayoutUnit grid, CoreText font-fallback cascade, Chrome line-breaking
  rules, table-cell wrap behavior.
- Corpus verification: 553 real-world flowcharts harvested from books,
  544 byte-identical (remaining cases categorized: rough.js randomness,
  sub-0.01px arc noise, one space-kern quirk).

#### `%%{init}%%` directive support
- Directive parsing (`init`/`initialize`, single-quote JSON tolerance).
- Full khroma 2.1.0 color-math port and the base/default/dark/forest
  theme classes (constructor + updateColors + override application),
  themed stylesheet generation with stylis semantics.
- `htmlLabels:false`: SVG `<text>`/`<tspan>` labels (createFormattedText
  port) with the 1/64px-grid `getComputedTextLength` rounding model.
- All 17 directive corpus cases byte-identical.

#### stateDiagram-v2
- Parser, stateDb (docTranslator, extract, dataFetcher) and rendering
  through a parametrized unified pipeline (per-diagram CSS, markers,
  aria roles): roundedRect state nodes, stateStart/stateEnd (rough.js
  ellipses with correctly-rounded CORE-MATH trig), notes with noteGroup
  clusters, the barb marker, markdown labels.
- 23 corpus cases: 20 byte-identical, 3 note diagrams identical modulo
  mermaid's own rough.js randomness.

#### sequenceDiagram
- Bespoke renderer port: bounds model, actor layout and margins from
  per-actor max message widths, messages/notes/loops, eight arrowhead
  defs and icon symbols, lifeline fixup, mirrored footer actors.
- The two-font measurement model (Times New Roman for layout, Trebuchet
  for drawn-text bboxes).
- 24 corpus cases, all byte-identical.

#### classDiagram
- Parser (members, methods, annotations, classifiers, relations with
  cardinalities), classBox compartment shape (textHelper layout, rough
  rectangle + divider lines), the 20-marker class set, cardinality
  terminal labels with upstream-faithful DOM ordering and the
  calcTerminalLabelPosition placement.
- 5 fixture cases, byte-identical modulo rough randomness.

#### timeline
- Parser (periods, events, sections, title) and renderer: timeline-node
  shapes with the d3 wrap algorithm (separator-keeping splits, collapsed
  whitespace measurement), f32 baseline accumulation, `4ex` title via
  x-glyph ink height, section color scales (double-run theme updates).
- 4 cases (1 corpus + 3 hand-made incl. sections), all byte-identical.

#### Infrastructure
- Blink-exact `getBBox`: f32 RectF cascade, dual attribute parsers
  (GenericParseNumber float accumulation vs CSS double lengths), Skia
  f32 cubic tight bounds, zero-area line unions.
- `seb`-style CLI (`-i input.mmd [-o output.svg] [--id svg-id]`) with
  diagram-type auto-detection.
- Test suites: dagre differential, 14 hand-written flowcharts, and the
  five diagram corpora with byte-identical regression guards.
