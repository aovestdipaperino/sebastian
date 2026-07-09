# sebastian-wasm

WASM bindings for [sebastian](../sebastian): render mermaid diagrams to SVG in
the browser or Node.js with no headless Chrome, no Puppeteer, and no
per-platform native binary.

## Build

```sh
cargo install wasm-pack   # once
wasm-pack build sebastian-wasm --target web       # browser (ES module)
wasm-pack build sebastian-wasm --target nodejs    # Node.js
```

This produces `sebastian-wasm/pkg/` with the `.wasm` binary, JS glue, and
TypeScript definitions — ready to `npm publish` or vendor directly.

## Fonts

Rendering works out of the box: sebastian embeds Cabin (standing in for
Trebuchet MS) and Tinos (Times-metric-compatible), both SIL OFL. Output with
the embedded fallbacks is well-proportioned but **not byte-exact vs mmdc**.

For pixel-perfect output, register the real font bytes before the first
render:

| file name | needed for |
|---|---|
| `Trebuchet MS.ttf` | most diagram types |
| `Trebuchet MS Bold.ttf` | bold labels, class/timeline titles |
| `Times New Roman.ttf` | sequence diagrams |
| `Verdana.ttf`, `Arial.ttf`, … | fallback glyphs (symbols, CJK) |

Trebuchet MS and Times New Roman are proprietary Microsoft/Apple fonts and are
not bundled; copy them from a machine that has them (macOS:
`/System/Library/Fonts/Supplemental/`, Windows: `C:\Windows\Fonts\`).

```js
register_font("Trebuchet MS.ttf", new Uint8Array(await (await fetch("fonts/Trebuchet MS.ttf")).arrayBuffer()));
```

One caveat: gantt date arithmetic runs in UTC on wasm (native builds use the
system timezone via libc, which wasm does not have).

## Browser demo

```sh
wasm-pack build sebastian-wasm --target web
mkdir -p sebastian-wasm/demo/fonts
cp "/System/Library/Fonts/Supplemental/Trebuchet MS.ttf" \
   "/System/Library/Fonts/Supplemental/Trebuchet MS Bold.ttf" \
   "/System/Library/Fonts/Supplemental/Times New Roman.ttf" \
   sebastian-wasm/demo/fonts/
python3 -m http.server -d sebastian-wasm 8000
# open http://localhost:8000/demo/
```

Type a mermaid diagram in the textarea and hit Render — the SVG is produced
entirely inside the page by the WASM module.

## API

```ts
init(): Promise<void>                              // load the wasm module (web target)
register_font(fileName: string, data: Uint8Array)  // call before render()
render(source: string, id: string): string         // mermaid source -> SVG, throws on parse errors
detect_diagram_type(source: string): string        // e.g. "flowchart", "sequence"
```
