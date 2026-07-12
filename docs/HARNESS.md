# Reference harness

Everything sebastian byte-diffs against comes from **mermaid 11.15.0**
rendered by mermaid-cli in headless Chrome. The toolchain that produces
those references — and that answers "what does Chrome actually do here?"
questions during porting — is rebuilt from scratch by one script:

```sh
scripts/setup-harness.sh            # → ./harness (gitignored)
scripts/setup-harness.sh --elk      # also build the ELK render bundle
```

## What you get

| Path | Purpose |
|---|---|
| `harness/node/node_modules/.bin/mmdc` | **The** reference renderer: mermaid-cli 11.12.0 pinned to mermaid 11.15.0. Homebrew's `mmdc` bundles a different mermaid and does *not* match the references. |
| `harness/mermaid/` | Sparse clone of the mermaid repo at tag `mermaid@11.15.0` — the readable TS/jison/langium sources to port from. |
| `harness/probe.mjs` | Template for probing ground truth (getBBox, `getComputedTextLength`, CSSOM serialization…) in the *same* headless Chrome that renders references. Edit the `page.evaluate` body per question. |
| `harness/elk/render.mjs` | (with `--elk`) Renders `layout: elk` diagrams exactly as mermaid does and captures the exact ELK-JSON mermaid feeds elkjs. Needed because mmdc does **not** auto-register `@mermaid-js/layout-elk`, and jsdom can't run mermaid's render at all (no `CSSStyleSheet`/`getBBox`). |

## Regenerating references

```sh
scripts/regen-refs.sh sebastian/tests/flowchart_cases sebastian/tests/state_cases
```

renders the `.svg` next to every `.mmd` in the given directories. Only do
this when bumping the mermaid version or adding fixtures, and review the
diff before committing:

- **rough.js shapes** (stadium `([])`, odd `>]`, er entity boxes, class
  boxes, requirement boxes) embed `Math.random()` in curve control points —
  those bytes churn on every render. The corpus tests mask them.
- **random ids** (gitGraph bare `commit`, anonymous `block`) also churn;
  fixtures use explicit ids for determinism.
- **gantt** embeds the render-time *today* marker and depends on the
  timezone: regenerate (and run tests) with `TZ=America/Los_Angeles`.

The corpus tests enforce byte-identity (with per-case masks for the above),
so `cargo test --workspace` after regeneration tells you exactly which
diagrams moved.

## The verification loop

The loop that got every diagram type to byte-exact (see `PORTING_NOTES.md`
and `FINDINGS.md` for what it uncovered):

1. Render the same `.mmd` with harness `mmdc` and with `seb`.
2. `cmp` the SVGs; look at the first differing byte in context.
3. That context names the JS/Chrome behavior to replicate — check
   `FINDINGS.md` first, it is probably already cataloged.
4. If the question is "what does Chrome compute here", answer it with
   `harness/probe.mjs` against the real headless Chrome, not from font
   tables or documentation.

## Version bumps

To target a newer mermaid: change `MERMAID_VERSION` (and, if needed, the
mermaid-cli pin) at the top of `scripts/setup-harness.sh`, rebuild the
harness, regenerate all reference dirs, and fix whatever the corpus tests
flag. Byte-exactness is defined against exactly one mermaid version at a
time — the README states which.
