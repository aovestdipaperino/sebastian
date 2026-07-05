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
- **requirement** (APPROXIMATE, non-byte-exact - reuses the unified dagre pipeline; smoke-tested)
- **C4** (`C4Context`/`Container`/`Component`/`Dynamic`/`Deployment`; APPROXIMATE, non-byte-exact - deterministic row-based layout; smoke-tested)

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

**Current status:** elk-flagged flowcharts render **approximately** today —
sebastian ignores the `elk` directive and lays them out with its byte-exact
dagre engine rather than erroring (smoke-tested; note mermaid itself falls back
to dagre when `@mermaid-js/layout-elk` is not registered).

**Reuse spike (2026-07, quantified).** Two native-Rust ELK ports exist —
`elkrs` (crates.io 0.1.1, Apache-2.0, byte-exact vs **ELK 0.11.0**) and
`openedges/elk-rs`. `@mermaid-js/layout-elk` pins **elkjs `^0.9.3`** (ELK 0.9.x).
I measured the actual divergence by running the *same* ELK-JSON graphs through
both engines (elkjs 0.9.3 in Node vs `elkrs` 0.11 via `layout_json`):
- **Acyclic layered graphs: BYTE-IDENTICAL** — every node coordinate and the
  container size matched to the full repeating decimal (e.g. `115.33333333333333`,
  `187.33333333333331`). The 0.9→0.11 placement change does **not** affect these.
- **Cyclic / back-edge graphs: DIVERGE** — a node x flipped `95.333` (elkjs) vs
  `78.666` (elkrs), i.e. cycle-breaking / Brandes-Köpf balancing changed between
  versions.
So `elkrs` IS reusable and byte-exact for the (large) DAG subset of mermaid ELK
flowcharts; only cyclic graphs need a 0.9.x-targeted engine for exactness.

**End-to-end proof (2026-07).** Beyond the synthetic-graph spike, I captured the
*exact* ELK-JSON mermaid feeds elkjs for a real flowchart (by hooking
`ELK.prototype.layout` in an esbuild bundle of `mermaid` + `@mermaid-js/layout-elk`,
run under puppeteer) and ran that identical input through `elkrs::layout_json`.
**Every node coordinate was byte-identical** to elkjs 0.9.3 (e.g.
`C: 138.5390625, 308.390625`; `E: 29.02734375, 397.390625`) — including mermaid's
node dimensions (which sebastian *already* measures byte-exact for flowchart) and
mermaid's actual layout options. So the ELK placement half is a solved problem.

Mermaid's layout options (from the capture), needed when building the ELK graph:
```
elk.hierarchyHandling: INCLUDE_CHILDREN
elk.algorithm:         elk.layered
nodePlacement.strategy: BRANDES_KOEPF
elk.layered.mergeEdges: false
elk.direction:         DOWN   (from the flow direction)
spacing.baseValue:     35
elk.layered.unnecessaryBendpoints: true
```

**Stage 1 (DONE, 2026-07): `elk` feature + byte-exact node placement, in-tree.**
`src/render/elk.rs` (behind the opt-in `elk` cargo feature) builds the ELK-JSON
exactly as mermaid does — the seven `layoutOptions` above, `children` with
measured `width`/`height`, and per-edge `labels` — runs it through
`elkrs::create_elk().layout_json`, and parses back node coordinates + edge
sections. `tests/elk_layout.rs` feeds the exact graph mermaid hands elkjs 0.9.3
and asserts node coordinates match **byte-for-byte**. Gotcha found and encoded:
ELK reserves an edge-label layer (which shifts between-layer spacing) only when
the label `text` is non-empty — so `ElkEdgeInput.label_text` must be threaded
through, not just the measured `label_width`/`label_height`.

**Stage 2 (DONE, 2026-07): wired into the flowchart pipeline.** `config.layout`
(from the top-level `layout` directive or `flowchart.defaultRenderer: "elk"`)
selects the engine; `dagre_render`'s layout step calls `layout_with_elk` (build
ELK inputs from the measured `RenderGraph`, run `elkrs`, set node centers = ELK
top-left + size/2, edge points from ELK sections) instead of dagre when
`layout == "elk"` and the `elk` feature is on. The dagre path is untouched
otherwise. `tests/elk_layout.rs` renders a `layout: elk` flowchart end-to-end;
`y` layer positions are byte-exact and `x` matches mermaid to ~1/128px.

**Remaining work:**
1. **Node-dimension gap (~1/128px, node-dependent — NOT a clean offset).**
   Investigated 2026-07: mermaid's *own* dagre and elk passes measure node WIDTH
   differently (heights agree). Comparing mmdc-dagre vs mermaid-elk-input widths:
   most nodes differ by exactly 1/128 (Alpha 177.25 vs 177.2421875; "B" 69.0625
   vs 69.0546875) but some agree exactly ("D" 69.8125 == 69.8125). sebastian's
   dagre width == mmdc-dagre width byte-exact (95.015625), so sebastian is *not*
   wrong — mermaid's `layout-elk` inserts nodes into a throwaway measuring `<g>`
   (no diagram CSS) and reads a subtly different `getBBox` width. Because the
   delta is node-dependent (0 or 1/128), a blanket `-1/128` correction is wrong
   (it would break the exact-match nodes). Byte-exact `x` needs modeling
   `layout-elk`'s *own* measuring-context `getBBox` — a second getBBox model for
   1/128px. Disproportionate; deferred. `y` (layer positions) is already exact,
   and node placement is exact when the input dims are exact (stage-1 test).
2. **Edge routing.** Edges currently use ELK's raw section points; sebastian's
   `insert_edge` then clips them. Straight edges already match mermaid to ~1/128;
   diamond-source edges are ~3px off. Algorithm fully mapped from layout-elk
   (`render-*.mjs` ~101910): `points = [src, ...bendPoints, dest]` (offset for
   nesting); if the source shape is a diamond, `unshift` the source node *center*;
   if the target is a diamond, `push` the target center; then
   `cutPathAtIntersect(points.reverse(), {x,y=node center, width: sw, height,
   padding}, isDiamond).reverse()` to clip the source end, then
   `cutPathAtIntersect(points, {…endNode…}, isDiamond)` for the target; then
   `insertEdge` with curveBasis. `cutPathAtIntersect`/`intersection`/
   `diamondIntersection` are the same funcs sebastian already has for dagre — the
   work is the assembly + reverse/clip/reverse + not double-clipping with
   sebastian's `insert_edge`. **Note:** even done perfectly this is *not*
   byte-exact until the node-dim gap (#1) is closed, since edge endpoints are node
   centers. So it's an approximate visual refinement (~3px on diamonds) unless #1
   is solved first — low value-per-effort; defer behind #1.
3. **Clusters/ports.** Subgraphs now **fall back to dagre** for the whole render
   (decided once in `render()` when any node `is_group`/has a parent) so the
   cluster box is correct rather than a broken zero-height rect — flat `layout:
   elk` graphs still use ELK. Native ELK cluster layout needs a *nested* ELK
   graph (`INCLUDE_CHILDREN`, children under parents, positions read back
   relative-to-parent) instead of sebastian's per-level cluster extraction — a
   rework of the recursion, deferred. Directions (TB/BT/LR/RL), self-loops, and
   multi-edges are handled by the flat ELK path (smoke-tested).

Only cyclic graphs risk placement divergence (0.9 vs 0.11 cycle-breaking).

**Reference-generation harness (working, in scratchpad):** mmdc does **not**
auto-register layout-elk and jsdom can't run mermaid's render (`CSSStyleSheet`/
`getBBox` missing). What works: `esbuild --bundle` an entry that imports mermaid +
`@mermaid-js/layout-elk` (pulls in elkjs's CommonJS GWT blob + d3), inject the
bundle via puppeteer `addScriptTag`, `registerLayoutLoaders`, then `render`. See
`scratchpad/mermaid-ref/harness/{elkentry,elkrender,elkcap2}.mjs` +
`scratchpad/elkprobe/` (elkrs runner).

## Not planned (for now)

- **mindmap / architecture** — now shipped as **approximate** (non-byte-exact)
  renderers; see "Already done" above. Byte-exact remains out of reach:
  mindmap's cose-bilkent and architecture's cytoscape-`fcose` are force-layout
  engines (the latter `Math.random`-seeded, non-deterministic even run-to-run),
  and no reusable Rust crate exists for either (spike found only ELK ports).
- **requirement / C4** — shipped as **approximate** (non-byte-exact); see
  "Already done". Byte-exact is **not tractable** — and a 2026-07 investigation
  proved the target values aren't even reproducible from ground-truth Chrome.
  Both size their boxes with `calculateTextDimensions` → `getBBox()` ink extents,
  `Math.round`ed per line, family selected between `sans-serif` and the diagram
  font. The reference box widths land verbatim in the output (requirement label
  `max-width`; C4 shape widths). I probed the **same** headless Chrome that
  generated the references (via puppeteer, replicating `drawSimpleText`'s
  tspan + the exact family-index selection) at the real default `fontSize: 16`:
  - `&lt;&lt;Requirement&gt;&gt;` → Chrome `getBBox` = **211** (Trebuchet) /
    **195** (sans-serif), but reference `max-width − 50` = **193** — *below even
    the sans-serif ink*.
  - Per-string, the implied font size scatters 13.2–15.5px; no single
    `(font, size)` model fits (`ID: 1` matches Trebuchet@16, `Verification: Test`
    matches ~14.4).

  So the emitted integers do not correspond to Chrome `getBBox` of the label
  strings by any rule — the measurement path carries a factor (memoized config
  defaults / a non-obvious size resolution) that is not a stable, reproducible
  target. A Blink-matching ink measurer would **still not** hit these values, so
  this is closed as intractable rather than "needs a subsystem". (requirement
  additionally draws its box with randomized roughjs strokes.)

## Process for each new type

Same loop that got flowcharts to byte-exact (see PORTING_NOTES.md):
harvest real diagrams into `tests/book_cases/`-style fixtures, render with
mmdc (mermaid 11.15.0) for references, byte-diff, fix, and add a corpus
test with an identical-count guard.
