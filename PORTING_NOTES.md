# Porting notes (mermaid 11.15.0 → Rust)

Working notes for the pixel-perfect port.

## Reference toolchain (rebuild on demand)

The reference SVGs are produced by mermaid-cli (`mmdc`) driving **mermaid
11.15.0**. The whole toolchain is rebuilt by `scripts/setup-harness.sh`
(pinned mmdc, sparse mermaid source clone, Chrome probe template, optional
ELK bundle) — see **docs/HARNESS.md**. References are regenerated with
`scripts/regen-refs.sh`. The manual equivalent of the mmdc install:

```
mkdir -p harness/node && cd $_
npm init -y && npm install @mermaid-js/mermaid-cli@11.12.0 mermaid@11.15.0
```

`mermaid-cli` depends on `mermaid@^11.0.2`, so npm dedupes to the top-level
`mermaid@11.15.0` — verify with `cat node_modules/mermaid/package.json | grep
version`. Render with `node_modules/.bin/mmdc -i x.mmd -o y.svg -p pup.json`
(a `{ "args": ["--no-sandbox"] }` puppeteer config), default id `my-svg`.
**Do NOT use Homebrew `mmdc` (11.12.0) — it does not match the references.**
For the readable TS/jison/langium sources, shallow-clone the repo at tag
`mermaid@11.15.0` with a sparse checkout of `packages/mermaid/src/diagrams/…`
and `packages/parser/src/language/…`.

Verification loop: render `.mmd` with mmdc and with `seb -i x.mmd -o y.svg`,
byte-diff, chase the first differing byte. Text metrics need the macOS system
`Trebuchet MS.ttf`.

## Status

Ported diagram types (each with a byte-diff corpus test under
`sebastian/tests/`; "modulo …" = randomness/render-time state mermaid itself
embeds, unmatchable by any port):

- **flowchart / graph** — dagre layout + full renderer (553-diagram corpus).
- **stateDiagram-v2**, **sequenceDiagram**, **classDiagram**, **timeline**.
- **pie**, **erDiagram**, **xychart-beta**, **gantt**, **journey**,
  **quadrantChart**.
- **gitGraph** (`LR`/`TB`/`BT`, incl. commit labels + tags).
- **packet-beta**, **radar-beta**, **sankey-beta**.
- **block-beta** (columns, space, spans, composites, classDef/style, edges).
- **treemap-beta** (d3 squarify layout).

See `README.md` for the fixture counts and per-type notes, `TODO.md` for the
remaining types and why each is blocked (C4 = Helvetica ink metrics; ELK =
engine scale; mindmap/architecture = non-deterministic force layouts;
kanban = tractable but unported; requirement = rough-masked), and
`FINDINGS.md` for the catalog of Chrome/V8/mermaid behaviors replicated.

## Original flowchart port (still current)

- `src/jsmap.rs`, `src/graphlib.rs`, `src/dagre/*` — dagre port, exact-match
  differential tests (17 fixtures).
- `src/flowchart/*` — lexer/parser/flowDb port.
- `src/text.rs` — Chrome-exact text measurement (Trebuchet MS, kerning,
  ceil-to-1/64 LayoutUnit, 200px wrap → exact 200 width + style switch).
- `src/render/*` — full renderer; `tests/flowchart_rendering.rs` asserts
  byte-identical output vs mmdc for 14 diagrams.
- Known non-determinism: rough.js shapes (stadium/odd) — mermaid itself embeds
  Math.random() in curve control points (collinear ⇒ pixel-identical).
  Rasterized diff vs mmdc is within mmdc's own run-to-run variance.
- Rasterization (`render::raster`, `raster` feature) is sebastian's own, not a
  port. For **pixel-perfect raster comparison against the mermaid TS output**,
  rasterize with `FontSource::System` so the emitted `trebuchet ms, …` stack
  resolves to the same installed faces mmdc's Chrome uses. The default
  `FontSource::Embedded` (bundled Cabin) is for portable, deterministic output
  and is NOT comparable pixel-for-pixel with mmdc.

Corpus verification (tests/book_corpus.rs, 553 diagrams from ../books):
544 byte-identical (including all 17 %%{init}%% directive cases — themes,
themeVariables, htmlLabels:false). Remaining: rough.js randomness (3);
sub-0.01px arc/extrema noise (5); Chrome space-kern quirk (book274, ≤2px —
Chrome applies space-adjacent kern values that differ from the font's kern
table; root cause unresolved).

Directive support (src/render/config.rs, khroma.rs, themes.rs, css.rs,
svg_label.rs):
- detectInit: %%{init|initialize: JSON}%% with ' → " replacement; merges
  theme, themeVariables, flowchart.{htmlLabels,nodeSpacing,rankSpacing,
  padding,wrappingWidth,curve}, top-level htmlLabels.
- Full khroma 2.1.0 port (lazy HSL/RGB channels, SASS mix, round-to-1e10);
  stringify emits rgb() when alpha==1 even with fractional channels.
- Theme classes base/default/dark/forest: constructor vars, updateColors(),
  calculate() applies themeVariables overrides before AND after update.
- Stylesheet generated from theme vars; stylis minifies spaces after commas
  only at paren depth 0. useGradient themes emit <linearGradient> defs.
- htmlLabels:false → edge/cluster labels via getEffectiveHtmlLabels
  (flowchart.htmlLabels ?? htmlLabels ?? true) render as SVG
  <text>/<tspan> (createFormattedText port). Node labelHelper reads the
  top-level htmlLabels: when false, node labels also render as SVG text
  (createText isNode=true, centerText=!isNode=false, centering via the
  `.node .label text { text-anchor: middle }` CSS rule), labelEl transform
  is translate(0, -bbox.height/2), and classDef CSS targets the shape
  elements (rect/polygon/ellipse/circle/path) instead of `> *`/`span`.
  Validated byte-exact against mmdc in tests/flowchart_nohtml_cases.
  Known sub-pixel gap: Chrome sizes SVG-text nodes from glyph ink extents
  (getBBox) while this port uses advance widths, so a few glyphs (e.g. "f")
  and multi-line heights differ by ≤0.07px / 1 f32 ULP (chain, multibr);
  invisible when rasterized.
- SVG getComputedTextLength = exact advance+kern sum rounded HALF-UP to the
  1/64px grid, per inner tspan (verified empirically: "Ti"=-85 units exact
  but "Timeline" rounds the total up 1 unit). HTML measurement is NOT
  rounded this way. Cluster SVG labels don't decode entities and aren't
  text-anchored middle; edge labels are, with a background rect (padding 2).
- SVG text bbox: ascent=round(1923·size/2048), descent=round(455·size/2048),
  first baseline at font-size, line advance 1.1em, f32-quantized.

Key Chrome/JS behaviors replicated (discovered during verification):
- getBBox/SVGRect is f32-backed → quantize all shape sizes and the viewBox.
- DOMPurify trims attribute values in mermaid's final sanitize pass.
- d3-shape line() rounds path data to 3 decimals (digits default).
- V8 float formatting resolves shortest-repr ties half-to-even.
- CSSStyleSheet round-trip converts hex colors to rgb(r, g, b) in classDef CSS.
- The label div's style attribute order depends on whether labelStyle ran first.
- XHTML void elements serialize as `<br />`.
- lodash uniqueId counter is shared across the whole render (incl. recursive
  cluster renders) — one UniqueId per layout call sequence.
- Labels go through innerHTML: entities decode, unknown tags unwrap
  (DOMPurify KEEP_CONTENT), `\n` and literal \n become `<br />`.
- DOMPurify strips non-standard attributes (cylinder's label-offset-y).
- CoreText fallback: LucidaGrande → ArialUnicode (skipped for box-drawing →
  Hiragino W3); Helvetica for U+2070-209F; AppleSymbols for U+2200-22FF;
  Apple Color Emoji for emoji-presentation chars at 1.25em advance.
- classDef label font-size drives label measurement (height = 1.5em).
- Chrome line breaking: spaces, after hyphen (not before digit), before
  ( [ { when the preceding char is non-alphanumeric (LB30); wrapped
  table-cells clamp to max-width, expanding to min-content if larger.
- mermaidAPI maps `color` → `fill` for classDef tspan rules.
- copy() in mermaid-graphlib calls graph.edges(node) — the argument is
  ignored, so ALL edges are scanned per node (affects edge order).

## Pipeline (mermaid side, flowchart → SVG)

1. `flowDb.getData()` → LayoutData { nodes, edges, config }.
   - subgraphs pushed first (reverse declaration order), then vertices.
   - node: { id, label (default = id), labelType:'text', parentId, padding: flowchart.padding (default 15? check config), shape (squareRect/roundedRect/...), cssClasses: 'default '+classes, look:'classic', isGroup }
   - edge: { id: getEdgeId = `L_{start}_{end}_{counter}`, arrowTypeStart/End ('none'/'arrow_point'/...), minlen: rawEdge.length, label, labelpos:'c', thickness: 'normal'|'thick'|'invisible', pattern: 'solid'|'dotted'|'dashed', classes: 'edge-thickness-normal edge-pattern-solid flowchart-link', curve: interpolate || flowchart.curve (default 'basis') }
2. `layout-algorithms/dagre/index.js render()`:
   - graphlib Graph {multigraph, compound}; setGraph { rankdir: direction, nodesep: nodeSpacing(50), ranksep: rankSpacing(50), marginx:8, marginy:8 }
   - insertMarkers(['point','circle','cross'], type='flowchart-v2', diagramId)
   - nodes added in data4Layout order; `{...node}` copies; setParent if parentId
   - self-loop edges (start==end) → split into 3 edges via two `labelRect` 10x10 nodes `{id}---{id}---1/2`, names `{id}-cyclic-special-0/1/2` (note: '2' has typo name `-cyc<lic-special-2`); edge1 keeps start arrow + no label, edgeMid plain, edge2 keeps end arrow + label? (NO — edge2.label = '' too; LABEL kept only on... edge1.label='' edgeMid labels cleared, edge2.label='' — wait, edge2 keeps `label`? source clears edge1.label and edge2.label... edgeMid keeps label? CHECK: edge1.label=''; edgeMid start/end labels cleared but NOT .label; edge2.label=''. So mid keeps the label.)
   - adjustClustersAndEdges(graph) (mermaid-graphlib.js):
     - for each cluster: descendants map; clusterDb[id] = { id: findNonClusterChild(id), clusterData: node }
     - mark externalConnections if an edge crosses cluster boundary (d1 XOR d2 over edges, per cluster)
     - anchor fix loop over clusterDb keys
     - rewrite edges touching clusters: v/w := getAnchorId; edge.fromCluster/toCluster = original id; remove+setEdge (same name)
     - extractor(): clusters WITHOUT externalConnections become `clusterNode:true` nodes with embedded sub-`graph` (dir flips TB↔LR unless clusterData.dir; nodesep/ranksep 50/50 margin 8 — then overridden in recursiveRender to parent ranksep+25, nodesep from parent); recursion
   - recursiveRender(): DOM groups in order: g.root > (g.clusters, g.edgePaths, g.edgeLabels, g.nodes)
     - insertNode for each non-cluster node (sizes node.width/height), recursive for clusterNode (then updateNodeBounds from bbox + setNodeElem; ranksep+25)
     - clusters that stay flat: clusterDb[id].node = node (NOT inserted yet)
     - insertEdgeLabel for every edge (sets edge.width/height from label bbox)
     - dagreLayout(graph)
     - sortNodesByHierarchy → positionNode (translate(x,y)); cluster nodes translate(x+diff-w/2, y-h/2-8); flat clusters → insertCluster (node.height += subGraphTitleTotalMargin (0 default))
     - edges: insertEdge(edgePaths, edge, clusterDb, type, startNode, endNode, id) + positionEdgeLabel
3. setupViewPortForSVG(svg, padding=flowchart.diagramPadding(8), 'flowchart', useMaxWidth)
   - viewBox = `${bbox.x-8} ${bbox.y-8} ${bbox.w+16} ${bbox.h+16}`; configureSvgSize sets width/height or max-width style.

## Node sizing (labelHelper, htmlLabels=true default)

- label measured via div getBoundingClientRect: font = themeVariables.fontFamily
  '"trebuchet ms", verdana, arial, sans-serif', fontSize 16px, line-height 1.5 → single line height = 24.
  div: display table-cell, white-space nowrap, max-width 200 (flowchart.wrappingWidth);
  if bbox.width == 200 → display table, break-spaces, width 200.
- labelEl transform: translate(-w/2, -h/2). foreignObject gets width/height attrs = bbox.
- squareRect: labelPaddingX = padding*2, labelPaddingY = padding; total = bbox + 2*labelPadding (max with node.width/height).
- roundedRect: labelPaddingX/Y = padding; rx=ry=themeVariables.radius(5).
- question: s = (bbox.w+padding) + (bbox.h+padding); polygon points [(s/2,0),(s,-s/2),(s/2,-s),(0,-s/2)], translate(-s/2+0.5, s/2).
- circle: r = bbox.width/2 + halfPadding (padding/2).
- stadium/others: see shape files (stadium uses roughjs path even in classic look, roughness=0 fillStyle solid).
- updateNodeBounds: node.width/height = element.getBBox() (shape element incl. stroke? getBBox excludes stroke).
- intersect functions: rect / polygon (+offsets by minX/minY & node w/h) / circle(ellipse).

## Edge rendering (insertEdge)

- points: replace first/last via tail.intersect(points[1])/head.intersect(points[len-2]) (after slicing off original first/last).
- toCluster/fromCluster: cutPathAtIntersect with clusterDb[..].node bounds (custom `intersection` fn).
- lineData filtered NaN y; fixCorners() pre-rounds right-angle corners (radius 5, a=2√2) for non-'rounded' curves.
- curve: flowchart default 'basis' → d3 curveBasis; getLineFunctionsWithOffset applies marker offsets (arrow_point: 4) to first/last point coordinates inside the d3 line() accessors (plus crowding adjustment if difference < markerHeight, extraRoom 1).
- path classes: ` edge-thickness-normal edge-pattern-solid flowchart-link` etc; id `${diagramId}_..wait: .attr('id', `${diagramId}-${edge.id}`)`; style from edge.style joined with ';'.
- data-points attr = btoa(JSON of post-intersect points); data-edge, data-et, data-id, data-look.
- markers: marker-start/end = url(#${diagramId}_flowchart-v2-pointEnd) etc (type = data4Layout.type = 'flowchart-v2').
- positionEdgeLabel: label translate(edge.x, edge.y+0) unless updatedPath (cluster-cut or label coord not in path) → calcLabelPosition (midpoint by arc length, roundNumber 5 decimals).
- isLabelCoordinateInPath: rounds all decimals in d and checks substring match of rounded x OR y.

## Markers (point/circle/cross), flowchart type string 'flowchart-v2'

- pointEnd: viewBox 0 0 10 10, refX 5 refY 5, markerUnits userSpaceOnUse, w/h 8, orient auto, path M 0 0 L 10 5 L 0 10 z, class arrowMarkerPath, style stroke-width:1; stroke-dasharray:1,0.
- pointStart: refX 4.5, path M 0 5 L 10 10 L 10 0 z. circleEnd: refX 11, circle cx5 cy5 r5. circleStart refX -1. crossEnd: viewBox 0 0 11 11 refX 12 refY 5.2, w/h 11, path M 1,1 l 9,9 M 10,1 l -9,9, stroke-width 2. crossStart refX -1.
- Also -margin variants inserted (unused for classic look).
- Insertion order in svg: appended to root g before nodes are inserted.

## Cluster (subgraph) rect

- width = max(node.width, labelBBox.width + node.padding); node.diff = (width-node.width)/2 - padding or -padding.
- rect x = node.x - width/2, y = node.y - height/2; label translate(node.x - bbox.w/2, node.y - node.h/2 + subGraphTitleTopMargin).
- after: node.width/height = rect bbox.

## Label position helpers

- calcLabelPosition: midpoint at totalDistance/2 via calculatePoint (roundNumber 5).
- calcTerminalLabelPosition: distance 25+10 along reversed/forward path, offset d=10+5, with angle (see utils.ts:398).

## Self-edge config quirks

- subGraphTitleTotalMargin default 0 (flowchart.subGraphTitleMargin {top:0,bottom:0}).

## TODO checks

- config defaults: flowchart.padding (15?), wrappingWidth 200, curve 'basis', htmlLabels true, fontSize? theme.
- theme-default variables (colors) + styles.ts CSS template → emit <style> exactly.
- d3 curveBasis exact output (port from d3-shape basis.js): emits M, L for <3 pts, else C segments; also two-point case L; verify trailing behaviors.
- getEdgeId format: `${prefix}_${start}_${end}_${counter}` — check utils getEdgeId.
- lodash uniqueId counter is per-page; mmdc renders once per page → our per-layout counter matches only if mermaid calls layout once (true for flowchart without clusterNode sub-renders? recursiveRender calls dagreLayout per clusterNode subgraph — SAME page counter continues!! Layout ctx must be shared across recursive renders in one diagram render.)
  → IMPORTANT: UniqueId instance must be created once per `render()`, threaded through all dagreLayout calls.
- flow.jison: port grammar by hand; flowDb.addVertex/addLink semantics; vertex text default = id; getEdges defaultInterpolate.
- mermaid svg scaffold: <svg id="..." class="flowchart" ...><style>...</style><g><marker defs...><g class="root">...
  Check getDiagramElement & actual mmdc output structure. aria-roledescription etc.

## stateDiagram-v2 (src/state/mod.rs, 2026-06-12)

Reuses the unified dagre pipeline (render_unified + DiagramChrome). Pipeline:
parse → docTranslator ([*] → parent_start/_end, divider grouping) → extract →
dataFetcher (nodeDb caching, domId = state-{id}-{graphItemCount} updated per
encounter, notes → noteGroup cluster + note node + dashed note-edge).
- Shapes: rect+rx/ry → roundedRect (padding 8); stateStart (circle r=7);
  stateEnd (rough.js ellipse pair + inner, style="" from cssStyles=[] being
  truthy); note (rough rectangle, RANDOM stroke controls → masked in tests).
- Markers: barb only, inside <defs>, id {id}_stateDiagram-barbEnd, offset 0.
- Labels are labelType=markdown: span class "nodeLabel markdown-node-label",
  markdownToHTML semantics (plain text + \n→<br/>, strips post-\n spaces).
- CSS: themed_statediagram_css = shared prefix + styles.js port + shared
  neo suffix; stylis strips `//`-comments; theme vars: transitionColor,
  stateLabelColor, stateBkg, altBackground(#f0f0f0/#555), specialStateColor,
  innerEndBackground(nodeBorder|primaryBorderColor), compositeBackground, etc.

Chrome float-semantics discoveries (apply to ALL diagrams):
- V8 Math.sin/cos are CORRECTLY ROUNDED (CORE-MATH) in current Chrome; the
  core-math crate matches. fdlibm ports and system libm are 1 ulp off at
  some angles (rough ellipse points exposed this).
- getBBox: Blink parses ALL attribute numbers with GenericParseNumber<float>
  — an f32-accumulating parser that is NOT correctly rounded (e.g.
  "799.7435607910156" → 799.74359130859375, not the nearer 799.74353…).
  Element boxes are gfx::RectF (f32 x/y/w/h); SkRect→RectF width is an f32
  subtraction; translate = f32 adds on x/y only; RectF::Union recomputes
  right() = f32(x+w) and stores width = f32(right-x). Path tight bounds are
  Skia: f32 control points, SkFindCubicExtrema (A=d-a+3(b-c), B=2(a-2b+c)
  with 2b as b+b, C=b-a; double discriminant), SkCubicCoeff::eval f32 Horner.
  All ported in render/bbox.rs (blink_float, RectF model).
- Puppeteer probing (node_modules in the rebuilt mermaid-ref harness) is the
  way to bisect browser float behavior: render minimal SVG structures and read
  getBBox/matrix values directly.


## sequenceDiagram (src/sequence/, 2026-06-12)

Bespoke renderer (no dagre): bounds model with sequenceItems stack,
boundMessage/drawNote/drawLoop, actor boxes prepended via d3 .lower()
(document order reversed; footer actors first). 24/24 corpus byte-exact.
- Fonts: runtime font families are UNDEFINED. Layout measurement happens in
  a detached svg (outside the styled #id element) → Chrome's DEFAULT font,
  Times New Roman: advance+kern on the 1/64 grid, Math.round; line height =
  integer asc(14)+desc(3) extended by glyph ink (TNR 'g' depth 442 units).
  Drawn-text getBBox (note heights, drawText valign accumulation) instead
  uses the CSS-inherited Trebuchet (19/line).
- drawText valign=center computes y per line lazily (running bbox sums);
  CSSOM style attributes always serialize LAST.

### DEVIATION: hand-drawn look (sebastian extension, 2026-06-18)

Upstream mermaid's sequence renderer ignores `look` entirely — `look: handDrawn`
only affects the flowchart and unified-renderer diagram types, never the legacy
`sequenceRenderer.ts` + `svgDraw.js`. There is therefore NO TS reference for
this; it is a sebastian-original extension, not a port.

When `config.is_hand_drawn()`, `draw_rect` emits an `hd_polygon` group and
straight segments go through a new `draw_segment` (`hd_edge_d`) helper, gated so
crisp output is byte-for-byte unchanged (the corpus test still passes). Applied
to: actor boxes, footer boxes, note boxes, straight message lines, loop borders.
Left crisp on purpose: self-message bezier curves, the loop label tab, the thin
`#999` lifelines (whose `y2` is resolved lazily by `fix_lifeline`, so a path
`d` can't be finalized at creation), and arrowhead markers. Each branch is
tagged `HAND-DRAWN EXTENSION` in `render.rs`; covered by
`tests/sequence_handdrawn.rs`.

## timeline (src/timeline/, 2026-06-12)

- d3 wrap splits on /(\s+|<br>)/ KEEPING separators → tspans contain
  multiplied spaces, but getComputedTextLength sees xml:space-collapsed
  text (measure the collapsed string).
- Multi-tspan text bbox: baselines accumulate in f32 (16, f32(+17.6), ...).
- Title font-size 4ex: Trebuchet has no OS/2 sxHeight — Blink falls back to
  the 'x' glyph ink height (1071 units); bold face metrics for the bbox.
- Theme cScale/git colors need updateColors TWICE (self-referential darken
  steps run on both passes; cScaleInv/cScaleLabel keep first-run values).

## classDiagram (src/classdiag/, 2026-06-12)

Unified dagre pipeline + classBox shape (textHelper port): annotation/
label/members/methods groups, renderExtraBox height adjustments (JS
`(bbox.height - PADDING/2) || 0` can go NEGATIVE for empty groups), rough
rectangle + rough divider lines (random → masked in tests).
- Label max-width = round(Times width of ClassMember.text) + 50, where
  .text keeps a BACKSLASH-escaped visibility ('\+String name') — markdown
  unescapes it for display, so the fo width is Trebuchet of '+String name'.
- Class title measured in BOLD Trebuchet (label g style font-weight:bolder).
- Cardinality terminals: 11px labels in g.edgeTerminals; endLabelLeft's fo
  sits OUTSIDE its inner g (upstream quirk); fo style="width:len*9px;
  height:12px". DOM order: all edge labels, then all startRight terminals,
  then all endLeft terminals (the upstream loop is unawaited async — labels
  group by microtask round-robin). Positions via calcTerminalLabelPosition
  (atan2/sin/cos through core-math).
- Markers: 20 class markers; extensionStart-margin is NOT inside <defs>.

## getBBox / parsing model (render/bbox.rs)

- SVG LENGTH attributes (rect/circle/line x/y/w/h/r) parse as doubles (CSS
  path) then f32 per coordinate; line sizes = f32(double difference) and
  bottom()/right() reconstructed by f32 adds (ties half-even).
- Path data and transform lists parse via Blink GenericParseNumber<float>
  (f32-accumulating, NOT correctly rounded).
- <line> boxes union even with zero area; empty foreignObjects don't.
