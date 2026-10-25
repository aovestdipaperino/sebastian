# pele_cases

Diagram sources from obsidianmd/pele (MIT, kepano): the Mermaid
documentation examples (`tests/corpus/*-docs.json`, files named `*-docsNNN`)
and diagram strings lifted from its ported Mermaid specs
(`tests/compat/*/*.spec.ts`, files named `*-specNNN`), rendered with the
pinned mermaid 11.15.0 harness (`scripts/setup-harness.sh`). Only cases that
render without error and produce identical bytes on two consecutive renders
are kept; rough.js and random-id cases (most class, requirement and gitGraph
examples) are therefore absent.

`KNOWN_FAILURES.txt` lists the cases sebastian does not match yet. The corpus
test treats it as a ratchet: a listed case that starts passing, or an
unlisted case that fails, fails the test.
