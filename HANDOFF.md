# Handoff: pele corpus, remaining failures

Repo: `/Users/enzo/Code/seb`, branch `main` (everything is pushed).
Crate `sebastian` (library) and `seb` (CLI) are a Rust port of mermaid that must
match **mermaid 11.15.0** (headless Chrome via mermaid-cli) **byte for byte**.
The current job: make the imported pele corpus (555 cases) match, one diagram
family at a time. State at handoff: **445 pass, 110 are known failures**.

Read this whole file before touching code. Everything you need is here or in
the files it points to.

## 1. The ratchet (how success is measured)

- Cases live in `sebastian/tests/pele_cases/<type>/<name>.mmd` with the
  reference render `<name>.svg` next to it (made by the real mermaid 11.15.0).
- `sebastian/tests/pele_corpus.rs` renders every `.mmd` with sebastian and
  byte-compares to the `.svg`.
- `sebastian/tests/pele_cases/KNOWN_FAILURES.txt` lists the cases that are
  allowed to fail. The test fails when:
  - a case NOT in the list fails (regression), or
  - a case IN the list starts passing (you must remove it).
- After any change run, from the repo root:

```sh
cargo build -p seb
python3 scripts/update-pele-known-failures.py   # rewrites KNOWN_FAILURES.txt; prints "N known failures"
git diff sebastian/tests/pele_cases/KNOWN_FAILURES.txt   # only lines you intended to remove may change
```

If a line was ADDED to that file, you caused a regression. Fix the code, do not
commit the addition. Only commit removals.

Failures by family right now:

| family | count | notes |
|---|---:|---|
| mindmap | 35 | approximate layout, see 4.1 |
| c4 | 23 | approximate layout, see 4.2 |
| venn | 22 | diagram not ported at all, see 4.3 |
| architecture | 9 | approximate layout, see 4.4 |
| flowchart | 6 | float-level noise only, see 4.5 |
| wardley | 4 | annotation-box text metrics |
| gantt | 3 | see 4.6 |
| xychart | 2 | not yet analysed |
| sequence | 2 | see 4.7 |
| timeline, state, sankey, block | 1 each | not yet analysed except state (title width) |

## 2. Tools

Run all of these from the repo root after `cargo build -p seb`.

- `python3 scripts/pele/classify-failures.py <family>`: for every known failure
  of a family prints `ERR <message>` (sebastian errors out) or `VB <ours> <ref>`
  (viewBox differs, usually a size/layout problem) or `body` (same viewBox,
  content differs). Start every investigation here.
- `python3 scripts/pele/first-diff.py <family>/<case> [width]`: renders our
  output, strips `<style>`, prints our bytes and the reference bytes around the
  first difference. Example: `python3 scripts/pele/first-diff.py wardley/wardley-docs001 300`.
- Render one case by hand: `target/debug/seb -i path/to/x.mmd -o /tmp/x.svg`.
- Generate reference SVGs for new `.mmd` files:
  `node scripts/pele/batch-render.mjs <dir>` (needs the harness, see
  `scripts/setup-harness.sh`; paths inside the script point at
  `/Users/enzo/Code/seb/harness/node`). `scripts/regen-refs.sh` is the older
  equivalent. Gantt references must be made with `TZ=America/Los_Angeles`.
- **Mermaid's own source is at `harness/mermaid/packages/mermaid/src`**
  (version 11.15.0). Port from it, not from memory and not from pele. Look at
  `diagrams/<type>/` and `rendering-util/rendering-elements/shapes/`.
  Other mermaid copies under `~/.npm/_npx/*/node_modules/mermaid` are 11.16 or
  11.17, so shape code can differ slightly; prefer the harness copy.
- `scripts/gen-themes.py` regenerates `sebastian/src/render/themes_gen.rs`
  (run `cargo fmt` afterwards).

## 3. Rules of the repo (follow them or the commit is rejected)

1. Byte-exact against 11.15.0 is the only goal. Never loosen a comparison.
   The mask in the test only covers the gantt "today" marker.
2. Before every commit:
   ```sh
   cargo fmt
   cargo clippy --all-targets -- -D warnings     # must print nothing
   cargo test                                    # must pass (the pele test takes ~20 s)
   ```
   A pre-commit hook also runs rustfmt and clippy.
3. Commit messages end with this trailer (blank line before it):
   `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
4. Commit small and often on `main`, then `git push`. Use `feat:`, `fix:`,
   `docs:` prefixes.
5. Match surrounding code style: same comment density and naming. Port mermaid
   algorithms faithfully and keep the upstream function name in the doc comment
   (for example "Port of `labelHelper`").
6. macOS: `sed -i` needs a backup suffix and chokes on `/` in patterns. Use
   `python3` or `perl -pi -e` for edits in shell, or the Edit tool.
7. The repo is indexed by tokensave. A Bash `grep` for a code symbol is denied
   by a hook; use the `tokensave_search` / `tokensave_context` tools (with
   `literal: true` for exact text) or read files directly.
8. Use `/private/tmp/...` scratch space, not the repo, for throwaway files.
9. Do not change `harness/`. Do not edit reference `.svg` files in
   `pele_cases` (they are ground truth).
10. Update `README.md` (diagram table, pele corpus note: counts) and
    `CHANGELOG.md` when a family goes to zero failures or a diagram type is
    added.

## 4. Per family guidance

General recipe for any family: classify, pick the failure with the smallest
diff, find the first differing byte, find the matching code in the harness
mermaid source, port it, re-run the ratchet, commit. Many cases share one root
cause, so fix by cause, not by file.

### 4.1 mindmap (35)
Sebastian draws a deterministic tidy tree; mermaid uses cytoscape
cose-bilkent (force layout). Every case differs in node positions. Before
investing, check whether mermaid's layout is deterministic for these inputs
(render the same `.mmd` twice with `batch-render.mjs`, which already refuses
unstable renders; the references that exist are therefore stable). If stable,
porting `layout-base` / `cose-base` / `cytoscape-cose-bilkent` exactly (they
are JS, in `harness/node/node_modules` or the mermaid dependency tree) is the
path; it is a large job, so plan it as its own multi-commit effort. Cheaper
wins first: non-position diffs (shapes, classes, icons, markdown labels) show
up as `body` in the classifier, fix those before layout.

### 4.2 c4 (23)
README says "byte-exact closed as intractable (box max-width doesn't match
Chrome getBBox)". Re-check that claim on the pele cases: run the classifier,
look at whether the mismatch is text wrapping/measurement (fixable with the
existing `TextMeasurer`, `sebastian/src/text.rs`) or genuinely `getBBox` of
`<text>` in the C4 renderer. Source: `diagrams/c4/c4Renderer.js`,
`svgDraw.js`.

### 4.3 venn (22)
Not ported: no `sebastian/src/venn`. Port `diagrams/venn/` from the harness
(parser, db, renderer). The layout uses venn.js (`venn.js` npm package, in
the harness node_modules): circle/overlap layout with an optimiser. Needs a
faithful port including the numeric optimiser (constrained MDS, then
`bestInitialLayout`, nelder-mead / conjugate gradient); float order of
operations matters for byte equality. Register the type in
`sebastian/src/lib.rs` and `sebastian/src/render/diagram.rs` the way
`wardley` and `eventmodeling` are registered (copy that wiring). Add a
`venn_corpus.rs`-style test only if useful; the pele test already covers it.

### 4.4 architecture (9)
fcose (cytoscape) layout, random-seeded upstream; check stability like 4.1.
Only a few cases may be stable; pick those.

### 4.5 flowchart (6 left, all float noise)
`flowchart-docs006/007/011/026`: viewBox `y` / height differ by about 4e-6
(Chrome reports the top of cylinder arcs as -0.0000038 instead of 0).
`docs051` (lined cylinder): bbox height is 0.0083 shorter in Chrome than the
exact arc extreme. `docs050` (tilted cylinder): viewBox x is -7.6e-6 instead
of 0. All come from Chrome/Skia's arc bounds in `getBBox`. Our model of that is
`sebastian/src/render/bbox.rs` (`path_bbox`, the `'a'` arm is a hack that adds
both vertical extremes). The Blink path: SVG arc endpoint to center
conversion in double, then Skia `arcTo` conics, `computeTightBounds` in f32.
Emulating it exactly would fix 5 to 6 cases at once; try to reproduce the
-3.8e-6 for a plain cylinder first (`flowchart-docs011`).

### 4.6 gantt (3)
`docs002`, `docs004`: references were rendered under a different DST offset
(labels differ by an hour); not fixable, leave in the list. `docs010`: needs
`dateFormat YYYY` style handling, `vert` tasks, `topAxis`, and `themeCSS`
(see `sebastian/src/gantt/mod.rs`).

### 4.7 sequence (2)
`docs017`: central connections (`()` markers on arrows) are not implemented.
`docs034`: glyph fallback widths (a `&#9829;` entity measures with a fallback
font; `SeqMeasurer` in `sebastian/src/sequence/render.rs` uses Times).

### 4.8 the rest
wardley (annotation-box text metrics), xychart (2), timeline (1), state (title
width is narrower than mermaid's; unknown font), sankey (1), block (1): run
the classifier and first-diff, then decide. Skip a case if the cause is
timezone, randomness or unavailable fonts, and say why in the commit message.

## 5. Where things are

- `sebastian/src/<type>/` one module per diagram type; shared rendering in
  `sebastian/src/render/` (`shapes.rs` node shapes, `edges.rs`, `clusters.rs`,
  `svg_label.rs` SVG text labels, `bbox.rs` Chrome getBBox model, `themes.rs`
  plus generated `themes_gen.rs`, `css.rs` stylesheet).
- `sebastian/src/diagram.rs` dispatches by diagram type and applies the shared
  front-matter, title and accessibility handling at `render_diagram_inner`.
- `sebastian/src/text.rs`: `TextMeasurer` (Trebuchet, with bold and italic
  faces via `set_bold` / `set_italic`) for most diagrams, `SeqMeasurer`
  (Times) for sequence.
- Numbers: JS number formatting is `crate::svg::js_num`; Chrome stores layout
  values as f32, so round with `f32q` where mermaid reads them back from the
  DOM (`getBBox`, `getBoundingClientRect`).
- `docs/PELE-PLAN.md` has the overall plan and the goal to also support mermaid
  12.1.0 (not started; needs a second pinned harness).
- GitHub tracking issues exist per family in `aovestdipaperino/sebastian`
  (`gh issue list`); comment progress counts there and close an issue when its
  family reaches zero.

## 6. Known pitfalls

- Shapes: when a flowchart shape name has no handler, rendering panics with
  "no such shape". Aliases live in `shape_alias` in
  `sebastian/src/flowchart/db.rs`; handlers are in `insert_node_shape` in
  `sebastian/src/render/shapes.rs`.
- The vertex counter in flowchart ids (`my-svg-flowchart-A-0`) increments on
  every `addVertex`, including `@{ ... }` shape data statements.
- A repeated `subgraph X` id adds to the first one (merge); it does not
  replace it.
- Markdown labels: only `**`, `__`, `*`, `_` inline styles are parsed
  (`sebastian/src/render/svg_label.rs`). HTML labels that wrap fall back to the
  plain measurement.
- Font Awesome `fa:fa-x` icons: widths come from a tiny table
  (`icon_advance` in `shapes.rs`); unknown icons measure 0.
- Some files can change on disk under you (formatter, hooks). Re-read before
  editing a file you read a while ago.

## 7. Suggested order for a smaller agent

1. Flowchart float noise (4.5): self-contained, fixes up to 6 cases.
2. `body`-type failures in wardley / xychart / timeline / sankey / block /
   state (4.8): small, independent.
3. Sequence `()` central connections (4.7).
4. c4 re-investigation (4.2).
5. venn port (4.3), then mindmap (4.1) and architecture (4.4) as larger efforts.

After each step: ratchet script, fmt, clippy, test, commit with trailer, push.
