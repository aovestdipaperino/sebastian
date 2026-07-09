# Findings

Everything sebastian discovered the hard way, in two parts:

1. **Rendering nuances** — Chrome/V8/mermaid behaviors found by byte-diffing
   sebastian's output against official mermaid-cli (mermaid 11.15.0, rendered
   by headless Chrome on macOS) and chasing the first differing byte. None of
   it is documented upstream; most of it is observable only at full output
   fidelity. Each item states the behavior sebastian replicates and, where
   useful, how it was pinned down.
2. **Environment & tooling** — toolchain, CI, font, and release-process
   gotchas: the kind of thing that costs an hour the second time you hit it.

## Part 1 — Rendering nuances

Everything listed here was discovered by byte-diffing sebastian's output
against official mermaid-cli (mermaid 11.15.0, rendered by headless Chrome
on macOS) and chasing the first differing byte. None of it is documented
upstream; most of it is observable only at full output fidelity. Each item
states the behavior sebastian replicates and, where useful, how it was
pinned down.

The single most effective debugging tool was driving Chrome directly with
puppeteer (`node probe.mjs` against minimal SVG fragments) to read back
`getBBox()`, parsed matrix values, and text measurements — bisecting
browser float behavior empirically instead of guessing from source.

### JavaScript / V8 numerics

- **Number-to-string formatting.** Attribute values are printed like JS
  `String(number)`: integers without a decimal point, otherwise the
  shortest representation that round-trips, with ties resolved
  half-to-even the way V8 does (Rust's default formatter differs on
  ties). Scientific notation only outside the exponent range (-7, 21).
- **`Math.sin`/`Math.cos` are correctly rounded.** Current V8 ships
  CORE-MATH implementations. System libm and fdlibm-derived ports (musl,
  the `libm` crate, V8's *old* ieee754.cc) differ in the last ulp at some
  angles. The rough.js ellipse in state diagrams exposed this; the
  `core-math` crate matches bit-for-bit. `Math.atan2` (cardinality label
  placement) likewise.
- **`Math.round` is `floor(x + 0.5)`**, not round-half-away-from-zero.
  Matters for text dimension rounding at exact `.5` values.
- **lodash `uniqueId` is shared across one whole render**, including
  recursive cluster sub-renders: a single counter instance must thread
  through every dagre layout call of a diagram.

### Chrome text measurement

- **HTML label width** (foreignObject `getBoundingClientRect`) is the raw
  advance sum, kerning included, ceiled to the 1/64px LayoutUnit grid.
- **SVG `getComputedTextLength`** rounds the advance+kern sum **half-up**
  to the 1/64px grid per tspan — HTML measurement does not. The same kern
  pair can therefore contribute differently depending on the rest of the
  string ("Ti" measures exactly; "Timeline" rounds the total up one unit).
- **SVG text whitespace collapses before measurement.** Timeline's d3
  `wrap` keeps whitespace tokens when splitting (`/(\s+|<br>)/` with a
  capture group), so the emitted tspans contain doubled/tripled spaces —
  but `getComputedTextLength` sees the xml:space-collapsed text.
- **SVG text bbox height** is the integer font box: `round(ascender)` +
  `round(descender)` from hhea, scaled (Trebuchet 16px → 15 + 4 = 19),
  *extended by glyph ink* that reaches beyond it (Times New Roman 'g'
  reaches 442/2048 units below baseline > the integer descent of 3).
- **Multi-tspan bbox baselines accumulate in f32**: first baseline at
  `1em`, each subsequent at `f32(prev + 1.1em)`. Three lines at 16px give
  54.19999694824219, not f32(54.2).
- **Sequence diagrams measure in Times New Roman.** The runtime font
  config is undefined (only sizes are set), and mermaid's measuring SVG is
  appended to `<body>`, *outside* the styled `#id` element — so layout
  measurement uses Chrome's default font, while the drawn text inherits
  Trebuchet from the stylesheet. Two fonts per diagram, both replicated.
- **Class diagrams measure the escape backslash.** Label wrap widths come
  from `ClassMember.text`, which stores the visibility prefix
  backslash-escaped (`\+String name`); markdown later unescapes it for
  display. The wrap width is `round(TimesWidth("\+String name")) + 50`,
  the rendered foreignObject width is the Trebuchet width of
  `+String name`. Class titles measure in **bold** Trebuchet (the label
  group carries `font-weight: bolder`).
- **`ex` units without an x-height metric.** Trebuchet has no OS/2
  `sxHeight`; Blink falls back to the ink height of the 'x' glyph (1071
  units). Timeline's `4ex` title resolves through that.
- **CoreText font fallback** for characters Trebuchet lacks: Lucida
  Grande → Arial Unicode (skipped for box-drawing, which goes to Hiragino
  Kaku Gothic W3), Helvetica for U+2070–209F, Apple Symbols for
  U+2200–22FF, Apple Color Emoji at a 1.25em advance.
- **Line breaking** follows Chrome: spaces, after hyphens (not before a
  digit), and before `(`/`[`/`{` when the preceding character is
  non-alphanumeric (UAX #14 LB30). A wrapped table-cell clamps at
  max-width and expands to min-content (longest unbreakable word) when
  that is larger.

### Blink getBBox

`SVGGraphicsElement.getBBox()` is not f64 geometry. The full pipeline:

- **Attribute parsing uses two different parsers.** Path data and
  transform lists go through Blink's `GenericParseNumber<float>` — a
  hand-rolled f32-accumulating parser (`integer += multiplier * digit`,
  `frac *= 0.1f`) that is *not* correctly rounded: `"799.7435607910156"`
  (an exact f32 midpoint) parses to the *upper* neighbor, where strtof
  ties to even (lower). SVG *length* attributes (rect/circle/line
  x/y/width/height/r) instead parse as doubles through the CSS path and
  are narrowed to f32 per coordinate.
- **Every element box is a `gfx::RectF`**: f32 x, y, width, height.
  `right()` is recomputed as `f32(x + w)` — when `x + w` lands exactly
  between two f32s, IEEE ties-to-even decides. `SkRect → RectF` makes the
  width an f32 *difference* of the f32 edges. Line sizes are f32s of
  *double* differences (`f32(423.4 - 117.8)` picks the upper neighbor even
  though both edges round down individually).
- **Translates offset boxes with f32 adds** on x/y only; size unchanged.
  Unions take f32 min/max of edges and re-derive width/height as f32
  differences. The cascade is observable: the same local box mapped
  through `translate(473.7410216331482, …)` lands on a different f32 than
  the f64 sum would.
- **Path tight bounds are Skia's** `computeTightBounds`: control points in
  f32, extrema via `SkFindCubicExtrema` (`A = d − a + 3(b − c)`,
  `B = 2(a − 2b + c)` with `2b` computed as `b + b`, `C = b − a`; the
  discriminant in double, everything else f32) and `SkCubicCoeff::eval`
  in f32 Horner form. Near-degenerate closing segments of rough ellipses
  make the coefficient rounding order observable.
- **`<line>` elements union even with zero area** (a horizontal line has
  height 0 but still extends the box); empty foreignObjects do not.
- **getBBox excludes** markers, strokes, `<defs>`, and HTML inside
  foreignObject (only the width/height attributes count).

### DOM and serialization

- **Chrome's XMLSerializer**: SVG empty elements self-close (`<rect/>`),
  XHTML void elements get a space (`<br />`), XHTML non-void elements
  always get explicit end tags.
- **DOMPurify trims attribute values** in mermaid's final sanitize pass,
  strips non-whitelisted attributes (e.g. the cylinder shape's
  `label-offset-y`), and unwraps unknown tags keeping their content.
- **Labels round-trip through `innerHTML`**: entities decode, `\n` and
  literal `\n` become `<br />`, markup is parsed and re-serialized (so
  `&quot;` in becomes `"` out, raw `&` becomes `&amp;`).
- **CSSOM-built `style` attributes serialize last**, regardless of when
  the first `.style()` call happened relative to `setAttribute` calls.
- **d3 `.lower()`** prepends, so sequence actors appear in the document in
  reverse draw order, footer actors first — before even the `<style>`
  element the renderer started with.
- **CSSStyleSheet round-trips hex colors to `rgb(r, g, b)`** in classDef
  CSS, and `color` maps to `fill` for tspan rules.
- **Unawaited async loops leave microtask fingerprints.** Class diagram
  cardinality terminals appear grouped by *type* (all labels, then all
  startRight terminals, then all endLeft terminals) because the upstream
  insertion loop doesn't await each edge: every async call runs to its
  first await synchronously, then resumes round-robin.
- **The `style` attribute of cross markers etc.** keeps mermaid's odd
  spacing (`stroke-width: 1; stroke-dasharray: 1, 0;`) — d3 sets it
  verbatim.

### stylis / themes / khroma

- **stylis minifies spaces after commas only at paren depth 0** — `rgba(232, 232, 232, 0.8)` keeps its spaces, font lists lose theirs.
  `//`-style line comments are stripped. Declarations placed after nested
  rules still merge into the parent rule, emitted before the nested ones.
  A trailing declaration without `;` gets one.
- **`updateColors()` runs twice** per theme resolution. Most variables are
  guarded (`x = x || …`) and idempotent, but the color-scale and git
  blocks contain unconditional self-referential steps
  (`cScale0 = darken(cScale0, 10)`), so the published values are
  double-darkened; `cScaleInv`/`cScaleLabel` capture first-run inputs.
  `cScaleLabel` only computes on the second pass because
  `labelTextColor` is still the literal string `'calculated'` during the
  first.
- **khroma**: lazy HSL↔RGB channel conversion, SASS-compatible `mix`,
  rounding to 1e10. `stringify` emits `rgb()` (not `rgba()`) when alpha
  is exactly 1 even with fractional channels, and hex only when all RGB
  channels are integers.
- **JS truthiness leaks into output**: `cssStyles` being an empty *array*
  is truthy, so every rough path gets `style=""`; `(bbox.height - 6) || 0`
  yields **-6** for an empty group, and that negative height flows into
  classBox layout when the empty-members box is rendered.

### rough.js (look: "classic", roughness 0)

- Solid fills are plain `M … L …` polygons (no `Z`); strokes are
  double-drawn per segment (`M a C lerp(t) lerp(2t) b` twice), where the
  divergence `t` is `Math.random()`-derived **in mermaid itself** — the
  control points are collinear, so geometry is identical but bytes differ
  run to run. Tests compare those payloads masked.
- Ellipses at roughness 0 take the `coreOnly` path: increment 2π/9
  divided by 4, points from correctly-rounded cos/sin with float-
  accumulated angles, a Catmull-Rom-style `_curve` with tightness 0, and
  `opsToPath` printing with JS number formatting. Deterministic — and
  byte-exact only with V8-identical trig.
- `rc.line` y-values get a `+0.001` offset upstream (so gradient strokes
  have a non-degenerate bbox); divider lines inherit it.

### mermaid quirks by diagram

#### flowchart
- Self-loops split into three edges via two invisible 10×10 `labelRect`
  nodes; the *middle* edge keeps the label. One generated name contains a
  literal typo (`-cyc<lic-special-…`) — replicated.
- `mermaid-graphlib.copy()` calls `graph.edges(node)` but the argument is
  ignored: all edges scan per node, which fixes the edge processing order.
- Edge label `<p>` content comes from markdownToHTML (marked 16) even for
  `labelType: 'text'` flowcharts via the default `markdown: true`.

#### stateDiagram-v2
- Rendered through the same unified dagre pipeline as flowchart-v2.
  `shape: 'rect'` with rx/ry set resolves to the *roundedRect* handler
  (theme radius 5), not the state shape registered under that name.
- `dataFetcher` re-creates node data on every encounter; the `domId`
  keeps the *last* graphItemCount (`state-Open-3` for a node first seen
  at counter 0).
- Node/edge labels are `labelType: 'markdown'`: the span class gains
  `markdown-node-label`, and `\n` plus following spaces collapse to
  `<br/>`.
- stateEnd draws two rough ellipses (outer 14, inner 5 = 14·5/14) and the
  empty-array-truthiness gives every path `style=""`.
- The `barb` marker lives inside its own `<defs>`; `markerUnits`
  userSpaceOnUse; offset 0 (but the offset code path still runs).

#### sequenceDiagram
- The drawn-text/getBBox accumulation in `drawText` (valign math, note
  rect heights) uses Trebuchet (19/line at 16px); the *model* dimensions
  (`calculateTextDimensions`) use Times New Roman (17/line). Both appear
  in the same coordinate computation.
- `drawText`'s per-line y is computed lazily — line N's y uses the summed
  bbox heights of lines < N, `Math.round`ed.
- Lifelines start at `y2=2000` and are fixed after layout
  (`fixLifeLineHeights`); the line's class is literally `actor-line 200`.
- Self-message curves are `C x+60,y-10 x+60,y+30 x,y+20`; bounds reserve
  `max(textWidth/2, width/2)` on both sides.
- The actor label `<text>` has no `font-family` style because the
  configured families are undefined at runtime — only sizes survive
  `setConf`.

#### classDiagram
- `classBox` group heights go through `(getBBox().height - PADDING/2) || 0`
  when the empty-members box renders — `-6` is a real, used value.
- Cardinality terminals are 11px (`.edgeTerminals` CSS) with
  `style="width: ${len*9}px; height: 12px"` on the foreignObject; the
  endLabelLeft foreignObject sits *outside* its `inner` group (upstream
  bug, replicated).
- `calcTerminalLabelPosition` walks 25 + markerSize along the path
  (reversed for end labels) and offsets perpendicular by
  `10 + markerSize/2`; `edge.arrowTypeStart ? 10 : 0` is JS truthiness on
  a string, so `'none'` still yields 10.
- Twenty markers are emitted whether used or not;
  `extensionStart-margin` is the only one *not* wrapped in `<defs>`.
- The relation path style is the literal `";;;"` (empty style arrays
  reduced with separators).

#### timeline
- `getVirtualNodeHeight` measures by appending a throwaway text element
  to the live SVG — i.e. with the inherited Trebuchet.
- Node height = bbox height + `fontSize·1.1·0.5` + padding, with the
  string-vs-number `fontSize.replace('px','')` coercion ending up
  numeric either way.
- The first task column starts at `50 + leftMargin(150)`; the activity
  line spans `leftMargin → boxWidth + 3·leftMargin` where `boxWidth` is
  the bbox *before* the title and activity line are added.

### fonts

Byte-exact output requires the actual macOS system fonts:

| Font | Used for |
| --- | --- |
| Trebuchet MS (+ Bold) | rendered labels, drawn-text bboxes, class titles |
| Times New Roman | sequence/class layout measurement (Chrome default font) |
| Helvetica, Arial Unicode, Apple Symbols, Hiragino, Apple Color Emoji | fallback cascade |

Widths come from advance + `kern` table sums (ttf-parser); note that
Chrome's *rendered* kerning of space-adjacent pairs differs from the
font's kern table in a way that remains unexplained (`book274`, ≤2px —
the one known unresolved discrepancy).

## Part 2 — Environment & tooling

### WASM port (2026-07)

- **`core-math` does not cross-compile to wasm.** The crate vendors the
  CORE-MATH C sources; building for `wasm32-unknown-unknown` invokes clang
  with `--target=wasm32-unknown-unknown`, which fails on every `.c` file
  because neither Apple nor Ubuntu clang ships wasm libc headers. There is no
  cargo feature to opt out of the C build. Hence `src/mathx.rs`: `core-math`
  natively, `libm` on wasm (not correctly-rounded, so final-ULP coordinate
  drift vs mmdc is possible there).
- **`std::time::SystemTime::now()` panics on `wasm32-unknown-unknown`**
  (the target has no clock). It compiles fine — it only aborts at runtime,
  so a green `cargo check --target wasm32-unknown-unknown` does not prove a
  diagram type works. The gantt today-marker hit this; fixed with
  `js_sys::Date::now()` behind `cfg(target_arch = "wasm32")`.
- **`libc` has no wasm equivalent for `localtime_r`/`mktime`**, and wasm has
  no system timezone at all. Gantt date math falls back to UTC civil
  arithmetic (the Hinnant `days_from_civil`/`civil_from_days` helpers were
  already in the module).
- **wasm runtime failures surface as `RuntimeError: unreachable`** with a
  mangled-symbol stack. Build with `--dev` (not release) to get readable
  frames — that's how the `SystemTime` panic was located.

### Fonts

- **Tinos moved from Apache 2.0 to SIL OFL on Google Fonts**, and the
  `google/fonts` GitHub repo no longer has an `apache/tinos/` directory
  (404). The reliable download path is the JSON manifest at
  `https://fonts.google.com/download/list?family=Tinos` (strip the `)]}'`
  XSSI prefix), which lists direct `fonts.gstatic.com` URLs and embeds the
  license text.
- **GitHub's macOS runners ship the full macOS Supplemental font set**
  (Trebuchet MS, Times New Roman, Verdana…), so byte-exact corpora can run
  in CI on `macos-latest`. Ubuntu runners have none of them — with the
  embedded fallbacks the suite no longer panics there, but metric-dependent
  assertions fail, so Linux CI can only build/clippy, not corpus-test.
- **Cabin was already in-repo** (embedded by the `raster` feature) and is a
  reasonable Trebuchet stand-in; Tinos is metric-compatible with Times New
  Roman. Both are SIL OFL, so they can be embedded in the published crate.

### Test corpus

- **The gantt corpus is timezone-sensitive**: references were generated in
  Pacific time and only pass under `TZ=America/Los_Angeles` (verified: fails
  in UTC and Europe/Rome, at a byte offset deep in the axis markup). Gantt
  date arithmetic is deliberately naive-local (matching dayjs in the
  rendering browser), so this is inherent, not a bug. CI pins the TZ.

### Toolchain / release process

- **Homebrew rust shadows rustup on this machine** (`/opt/homebrew/bin`
  precedes `~/.cargo/bin`), and `rustup target add` only installs std for the
  rustup toolchain. Even `rustup run stable cargo` invokes plain `rustc` from
  PATH (Homebrew's). Wasm builds need the rustup toolchain forced, e.g.
  `PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
  wasm-pack build …` — the same applies to `wasm-pack`, which checks
  `rustc`'s sysroot.
- **GitHub Actions resolves secrets when the workflow run is created**, not
  when a job starts. A secret added while a run is in progress is invisible
  to that run's remaining jobs; `gh run rerun --failed` picks it up.
- **`cargo publish` order matters for path+version workspace deps**: `seb`
  depends on `sebastian` by version, so `sebastian` must be published first.
  cargo ≥1.66 waits for crates.io index propagation automatically, so no
  sleep is needed between the two publishes.

