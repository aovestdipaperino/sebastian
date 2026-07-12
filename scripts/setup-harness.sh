#!/usr/bin/env bash
# Rebuilds the mermaid reference toolchain that generates and verifies the
# byte-exact corpus references. Everything lands under one directory (default
# ./harness at the repo root, or $1); nothing outside it is touched. Safe to
# re-run — steps that are already done are skipped.
#
#   scripts/setup-harness.sh [target-dir]
#
# What it sets up:
#   <dir>/node/        npm project with the pinned mmdc (mermaid-cli 11.12.0
#                      driving mermaid 11.15.0) — THE reference renderer.
#                      Homebrew mmdc is 11.12 and does NOT match references.
#   <dir>/mermaid/     shallow sparse clone of mermaid at tag mermaid@11.15.0
#                      (the readable TS/jison/langium sources to port from).
#   <dir>/pup.json     puppeteer config for mmdc.
#   <dir>/probe.mjs    template for probing ground truth in the SAME headless
#                      Chrome that renders references (getBBox etc.).
#   <dir>/elk/         (optional, --elk) esbuild bundle of mermaid +
#                      @mermaid-js/layout-elk and a puppeteer runner, for ELK
#                      references and for capturing the exact ELK-JSON mermaid
#                      feeds elkjs. mmdc does NOT auto-register layout-elk,
#                      and jsdom can't run mermaid's render — this is the only
#                      known working path (see docs/HARNESS.md).
#
# After setup, regenerate references with scripts/regen-refs.sh.
set -euo pipefail

MERMAID_VERSION="11.15.0"
MERMAID_CLI_VERSION="11.12.0"

want_elk=0
dir=""
for arg in "$@"; do
  case "$arg" in
    --elk) want_elk=1 ;;
    *) dir="$arg" ;;
  esac
done
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="${dir:-$root/harness}"
mkdir -p "$dir"
dir="$(cd "$dir" && pwd)"
echo "harness dir: $dir"

# --- pinned mmdc ------------------------------------------------------------
if [ ! -x "$dir/node/node_modules/.bin/mmdc" ]; then
  echo "installing mermaid-cli $MERMAID_CLI_VERSION + mermaid $MERMAID_VERSION ..."
  mkdir -p "$dir/node"
  (cd "$dir/node" && npm init -y >/dev/null \
    && npm install --no-fund --no-audit \
         "@mermaid-js/mermaid-cli@$MERMAID_CLI_VERSION" "mermaid@$MERMAID_VERSION")
fi
got="$(node -p "require('$dir/node/node_modules/mermaid/package.json').version")"
if [ "$got" != "$MERMAID_VERSION" ]; then
  echo "ERROR: mmdc resolved mermaid@$got, need $MERMAID_VERSION" >&2
  echo "(mermaid-cli depends on mermaid@^11.0.2; npm must dedupe to the top-level pin)" >&2
  exit 1
fi
echo "mmdc OK (mermaid $got)"

cat > "$dir/pup.json" <<'EOF'
{ "args": ["--no-sandbox"] }
EOF

# --- mermaid sources --------------------------------------------------------
if [ ! -d "$dir/mermaid/.git" ]; then
  echo "cloning mermaid sources at mermaid@$MERMAID_VERSION (sparse) ..."
  git clone --depth 1 --branch "mermaid@$MERMAID_VERSION" --filter=blob:none --sparse \
    https://github.com/mermaid-js/mermaid.git "$dir/mermaid"
  (cd "$dir/mermaid" && git sparse-checkout set \
    packages/mermaid/src packages/parser/src)
fi
echo "mermaid sources OK"

# --- Chrome ground-truth probe template --------------------------------------
if [ ! -f "$dir/probe.mjs" ]; then
  cat > "$dir/probe.mjs" <<'EOF'
// Probe ground truth in the SAME headless Chrome that renders the references
// (mmdc's bundled puppeteer). Edit the page.evaluate body for the question at
// hand — getBBox of a string, getComputedTextLength, CSSOM serialization, ...
//   node probe.mjs
import puppeteer from './node/node_modules/puppeteer/lib/esm/puppeteer/puppeteer.js';

const browser = await puppeteer.launch({ args: ['--no-sandbox'] });
const page = await browser.newPage();
const result = await page.evaluate(() => {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  document.body.appendChild(svg);
  const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
  text.style.fontFamily = '"trebuchet ms", verdana, arial, sans-serif';
  text.style.fontSize = '16px';
  text.textContent = 'example string';
  svg.appendChild(text);
  const b = text.getBBox();
  return { width: b.width, height: b.height, ctl: text.getComputedTextLength() };
});
console.log(JSON.stringify(result));
await browser.close();
EOF
fi
echo "probe template OK"

# --- optional ELK bundle ------------------------------------------------------
if [ "$want_elk" = 1 ]; then
  if [ ! -f "$dir/elk/bundle.js" ]; then
    echo "building ELK render bundle ..."
    mkdir -p "$dir/elk"
    (cd "$dir/elk" && npm init -y >/dev/null \
      && npm install --no-fund --no-audit \
           "mermaid@$MERMAID_VERSION" "@mermaid-js/layout-elk@0.1.7" esbuild)
    cat > "$dir/elk/entry.mjs" <<'EOF'
import mermaid from 'mermaid';
import elkLayouts from '@mermaid-js/layout-elk';
window.mermaid = mermaid;
window.elkLayouts = elkLayouts;
// Hook ELK to capture the exact ELK-JSON mermaid feeds elkjs.
import ELK from 'elkjs/lib/elk.bundled.js';
const origLayout = ELK.prototype.layout;
window.__elkInputs = [];
ELK.prototype.layout = function (graph, options) {
  window.__elkInputs.push(JSON.parse(JSON.stringify(graph)));
  return origLayout.call(this, graph, options);
};
EOF
    (cd "$dir/elk" && node_modules/.bin/esbuild --bundle --format=iife entry.mjs \
       --outfile=bundle.js)
    cat > "$dir/elk/render.mjs" <<'EOF'
// Render a .mmd with layout:elk exactly as mermaid does, and dump both the
// SVG and the captured ELK-JSON inputs.
//   node render.mjs diagram.mmd out.svg [elk-inputs.json]
import { readFileSync, writeFileSync } from 'node:fs';
import puppeteer from '../node/node_modules/puppeteer/lib/esm/puppeteer/puppeteer.js';

const [src, out, elkOut] = process.argv.slice(2);
const bundle = readFileSync(new URL('./bundle.js', import.meta.url), 'utf8');
const mmd = readFileSync(src, 'utf8');

const browser = await puppeteer.launch({ args: ['--no-sandbox'] });
const page = await browser.newPage();
await page.addScriptTag({ content: bundle });
const svg = await page.evaluate(async (source) => {
  mermaid.registerLayoutLoaders(elkLayouts);
  mermaid.initialize({ startOnLoad: false });
  const { svg } = await mermaid.render('my-svg', source);
  return svg;
}, mmd);
writeFileSync(out, svg);
if (elkOut) {
  const inputs = await page.evaluate(() => window.__elkInputs);
  writeFileSync(elkOut, JSON.stringify(inputs, null, 2));
}
await browser.close();
EOF
  fi
  echo "ELK bundle OK"
fi

echo
echo "Done. Render references with:"
echo "  scripts/regen-refs.sh <dir-with-.mmd-files>   # uses $dir"
