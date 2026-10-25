# Plan: what to take from obsidianmd/pele

Pele (MIT, kepano) is a TypeScript Mermaid 12.1.0 reimplementation. It is not
byte-exact with Mermaid, so nothing replaces sebastian's core. It is useful as
(a) a reference for diagram types sebastian lacks, (b) a second opinion on the
four approximate renderers, (c) a source of ported upstream spec cases.

## Findings that shape the plan

- Mermaid 11.15.0 (our reference version) already ships **ishikawa, venn,
  wardley, treeView, eventmodeling, info**. These are real gaps, and byte-exact
  references can be generated from the pinned harness.
- **cynefin, railroad, usecase, swimlane, agentflow** are not in 11.15.0.
  Treat as optional extensions behind `mermaid-extensions`, like pyramid.
- Pele's parsers are faithful Jison ports (token-for-token), so they are good
  maps for our Rust parsers; its renderers are not the byte-exact target.
- Homebrew `mmdc` is 11.12.0-bundled and cannot render these types. Ground
  truth requires `scripts/setup-harness.sh` (mermaid 11.15.0 sources + mmdc).

## Phases

### Phase 0: harness
Run `scripts/setup-harness.sh`. Confirm it renders an ishikawa, venn,
wardley, treeView and eventmodeling sample. Record which render in 11.15.0
(some may be `-beta` keywords or need `look` config).
Exit: reference SVGs exist for every new type.

### Phase 1: ishikawa (smallest, proves the pipeline)
- `sebastian/src/ishikawa/mod.rs`: port pele's lexer/db/parser (about 6 KB),
  then port Mermaid's own renderer from `harness/mermaid` (not pele's) for
  byte-exactness.
- Detect `ishikawa` / `ishikawa-beta` in `diagram.rs`.
- Fixtures in `sebastian/tests/ishikawa_cases/`, corpus test like
  `packet_corpus.rs`, README table row.
Exit: fixtures byte-identical.

### Phase 2: treeView, then venn, then wardley, then eventmodeling, info
One type per commit, same recipe as phase 1. Order by size and reuse:
treeView (text layout), info (trivial), venn (needs a circle-overlap layout,
pele's `venn/layout.ts` is a readable reference), wardley, eventmodeling.
Exit: each type byte-exact or documented as approximate with reason.

### Phase 3: approximate renderers vs pele
Render the same inputs through sebastian and pele (pele via `npx`, scratch
dir only) for mindmap, architecture, C4, requirement. Compare against mmdc
output visually. Adopt pele ideas only where sebastian is clearly worse
(candidates: architecture group/junction handling, mindmap indented layout
option). Keep sebastian's own layouts otherwise.
Exit: written comparison in FINDINGS.md plus targeted fixes.

### Phase 4: test inputs and error positions
- Mine `tests/compat/*` from pele for extra .mmd inputs (C4 has 13, mindmap 8,
  venn 7) and add them as smoke or corpus cases.
- Check sebastian's syntax-error line/column against pele's recent fixes
  (entity codes, front matter, comments).

### Phase 5 (optional): pele-only extensions
cynefin, railroad, usecase, swimlane, agentflow behind `mermaid-extensions`,
only if wanted. Original renderers, no byte-exact claim.

### Goal: support mermaid 12.1.0
Mermaid 11.15.0 stays the byte-exact baseline until a 12.1.0 harness exists.
Then, in order:
1. Add a second pinned harness (`scripts/setup-harness.sh` option for 12.1.0)
   and render every corpus source with it into a separate reference set.
2. Diff the 11.15.0 and 12.1.0 references per diagram type to list what
   changed (CSS, theme variables, shapes, layout, new diagram types).
3. Make the renderer version-selectable (config or `--mermaid-version`),
   defaulting to 11.15.0, and port the 12.1.0 deltas behind it.
4. Run the pele corpus against both reference sets, each with its own
   `KNOWN_FAILURES` ratchet.
5. Decide per new type (cynefin, railroad, usecase, swimlane, agentflow)
   whether 12.1.0 ships it; if so, port it from the harness source.
Exit: the corpus matches 12.1.0 byte for byte at the level 11.15.0 does today,
and the README states which versions are supported.

## Rules
- Attribute pele (MIT, kepano) in a NOTICE entry for any ported logic.
- Byte-exact target is mermaid 11.15.0 from the harness (12.1.0 once its
  harness lands, see the goal above), never pele.
- Each type: fixtures, corpus test, README row, CHANGELOG entry, clippy clean.
