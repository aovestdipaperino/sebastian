# TODO — diagram types to support

Priority is demand-per-effort: measured demand in the `../books` corpus
(859 mermaid blocks scanned), general mermaid popularity, and how much of
the existing engine (dagre layout, Chrome-exact text measurement, themes,
SVG serializer) each type reuses.

Already done:
- **flowchart / graph** (781 corpus blocks, byte-exact vs mmdc)
- **stateDiagram-v2** (29 corpus cases: 23 byte-exact, 6 rough/randid-masked)
- **sequenceDiagram** (37 corpus cases, all byte-exact - incl. blocks, activations, autonumber, boxes, actor figures)
- **classDiagram** (9 hand-made cases, byte-exact modulo rough randomness - incl. generics, notes, namespaces, lollipop)
- **timeline** (4 cases, byte-exact)
- **pie** (2 cases, byte-exact)
- **erDiagram** (2 cases, byte-exact / rough-masked)
- **xychart-beta** (2 cases, byte-exact)
- **gantt** (5 cases, byte-exact modulo the render-time today marker)
- **gitGraph** (`LR`/`TB`/`BT`; 10 cases, byte-exact modulo random commit ids + 1-ulp viewBox/max-width; incl. commit labels + tags)
- **journey** (2 cases, byte-exact)
- **quadrantChart** (2 cases, byte-exact)
- **packet / packet-beta** (3 cases, byte-exact - incl. multi-row wrap, +bits, no-title)
- **radar / radar-beta** (3 cases, byte-exact - incl. circle/polygon graticule, axis-ref entries, options)
- **sankey / sankey-beta** (3 cases, byte-exact - full d3-sankey iterative layout; labels-within-bounds cases)
- **block / block-beta** (12 cases, byte-exact - columns, space, `:N` spans, nested composites, classDef/class/style, and edges incl. labels)
- **treemap / treemap-beta** (4 cases, byte-exact - d3 squarify layout, sections/leaves, font-shrink labels)
- **kanban** (3 cases, byte-exact - mindmap-indent parser, section clusters + item cards, column layout)
- **mindmap** (APPROXIMATE, non-byte-exact - deterministic tidy-tree; smoke-tested)
- **architecture** (APPROXIMATE, non-byte-exact - deterministic directional grid; smoke-tested)

## 1. stateDiagram-v2 — 28 corpus blocks

Highest reuse by far: mermaid renders state-v2 through the *same*
dagre-wrapper pipeline as flowchart-v2 (same `insertNode`/`insertEdge`,
clusters for composite states, same markers and label code). Mostly a new
parser plus a handful of shapes (start/end dots, fork/join bars, notes,
choice diamond). Cheapest path to a second byte-exact diagram type.

## 2. sequenceDiagram — 45 corpus blocks

Highest demand here and the #2 mermaid type everywhere. No dagre — its
layout is a bespoke message/actor bounds algorithm — but it leans heavily
on text measurement (actor boxes, message widths, wrapped notes), which is
the part that was genuinely hard and is already done. New work: the
sequenceDb parser, the bounds/loop-box layout, actor lifelines, activation
rects, and its own arrow markers.

## 3. classDiagram — 0 corpus blocks, but top-3 in the wild

Also rendered through the shared dagre pipeline (v11 unified renderer).
New work is mostly the parser and the multi-compartment class shape
(title / attributes / methods with dividers), plus relationship markers
(triangle, diamond, etc.) and cardinality edge labels.

## 4. erDiagram — popular in docs/architecture writing

Dagre-based layout as well. Entity boxes are row tables (name/type/key
columns — the table min-content measurement already exists), plus
crow's-foot markers and dashed/solid relationship styling.

## 5. xychart-beta — 3 corpus blocks

Self-contained: no graph layout at all, just d3 linear/band scales mapped
to bars and line points inside a plot area. Small, and it has actual
corpus demand with byte-diffable references.

## 6. pie — trivial, common in READMEs

One d3 arc loop, percentage labels, theme color cycle. A weekend-sized
diagram type; worth doing early purely for coverage breadth.

## 7. timeline — 1 corpus block

Column-of-events layout with wrapped text blocks; reuses text measurement
and theme section colors. Modest effort, low demand.

## 8. gantt

d3 time scales, axis tick generation, and date parsing (dayjs semantics)
make byte-exactness fiddlier than the value here suggests. Defer until
the above are done.

## Flowchart ELK layout — scoped, not started

`%%{init: {"flowchart": {"defaultRenderer": "elk"}}}%%` routes layout through
elkjs — a 1.5 MB GWT transpilation of the Java ELK *layered* engine
(network-simplex layering, Forster-constrained crossing minimization,
ELK's modified Brandes-Köpf placement, orthogonal edge routing, port
constraints). The mermaid side (`@mermaid-js/layout-elk`: render.ts,
geometry.ts, ~1.3k lines) is small; the engine is the project — a port
larger than the original dagre port, best done from the readable Java
sources (eclipse/elk) with differential fixtures, in its own multi-session
effort. Reference fixture harness: /tmp/gapcases/elk100.* pattern.

**Reuse spike (2026-07):** two native-Rust ELK ports exist — `elkrs`
(crates.io 0.1.1, Apache-2.0, byte-exact vs **ELK 0.11.0**) and
`openedges/elk-rs` (EPL-2.0, "drop-in elkjs replacement"). BUT
`@mermaid-js/layout-elk` pins **elkjs `^0.9.3`** (ELK 0.9.x), and ELK's
placement/spacing changed between 0.9 and 0.11 — so `elkrs` is NOT byte-exact
with mermaid's output as-is. Direct reuse would need a 0.9.x target in elkrs;
otherwise it only yields an *approximate* ELK layout. Even with a matching
engine, the `@mermaid-js/layout-elk` glue (render.ts/geometry.ts, node↔ELK-JSON
mapping + edge routing/label placement) still needs porting.

## Not planned (for now)

- **mindmap / architecture** — now shipped as **approximate** (non-byte-exact)
  renderers; see "Already done" above. Byte-exact remains out of reach:
  mindmap's cose-bilkent and architecture's cytoscape-`fcose` are force-layout
  engines (the latter `Math.random`-seeded, non-deterministic even run-to-run),
  and no reusable Rust crate exists for either (spike found only ELK ports).
- **requirement** — only byte-exact *modulo rough.js*. Drives the unified
  `render()` pipeline (reusable), BUT the `requirementBox` shape draws its box
  with roughjs curved double-strokes whose control points are randomized
  (verified in the reference), so box paths can't be matched byte-for-byte
  (same masking as er/state/class). Also uses `calculateTextWidth` (Arial ink,
  the C4 problem) as a `+50`-slack wrap hint (harmless for short text). A large
  port for a rough-masked result.
- **C4** — blocked on text metrics, not effort. C4's `calculateTextDimensions`
  measures with `getBBox()` **ink extents** (not advance widths), `Math.round`ed,
  taking the max over `sans-serif` and the (uninstalled) `"Open Sans"` family —
  so on the reference machine every measured width is Helvetica/Arial glyph-ink.
  The engine only models Trebuchet *advances*, so byte-exact C4 needs a whole new
  Helvetica ink-extent glyph-metrics subsystem, and the result would be fragile
  (the fallback face is environment-dependent). Parser/db/svgDraw (~2.2k loc) are
  straightforward; the measurement is the wall. Revisit only alongside a general
  multi-font ink-measurement effort.

## Process for each new type

Same loop that got flowcharts to byte-exact (see PORTING_NOTES.md):
harvest real diagrams into `tests/book_cases/`-style fixtures, render with
mmdc (mermaid 11.15.0) for references, byte-diff, fix, and add a corpus
test with an identical-count guard.
