# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- Renamed the project to **sebastian** (the crab from the mermaid) and
  split it into a workspace: the `sebastian` library crate and the `seb`
  CLI crate.

### Added
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
