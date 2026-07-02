# TODO — diagram types to support

Priority is demand-per-effort: measured demand in the `../books` corpus
(859 mermaid blocks scanned), general mermaid popularity, and how much of
the existing engine (dagre layout, Chrome-exact text measurement, themes,
SVG serializer) each type reuses.

Already done:
- **flowchart / graph** (781 corpus blocks, byte-exact vs mmdc)
- **stateDiagram-v2** (29 corpus cases: 23 byte-exact, 6 rough/randid-masked)
- **sequenceDiagram** (34 corpus cases, all byte-exact - incl. blocks, activations, autonumber, boxes, actor figures)
- **classDiagram** (9 hand-made cases, byte-exact modulo rough randomness - incl. generics, notes, namespaces, lollipop)
- **timeline** (4 cases, byte-exact)

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

## Not planned (for now)

- **mindmap / architecture** — depend on cose-bilkent / cytoscape force
  layouts; non-deterministic and a large port for niche demand.
- **gitGraph, journey, quadrant, C4, sankey, block, packet, kanban,
  requirement, radar, treemap** — no corpus demand; revisit if fixtures
  show up.

## Process for each new type

Same loop that got flowcharts to byte-exact (see PORTING_NOTES.md):
harvest real diagrams into `tests/book_cases/`-style fixtures, render with
mmdc (mermaid 11.15.0) for references, byte-diff, fix, and add a corpus
test with an identical-count guard.
