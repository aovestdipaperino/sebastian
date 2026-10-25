// usage: node batch-render.mjs <root dir>  : renders every .mmd twice, writes .svg when stable and not an error diagram
import fs from 'node:fs'; import path from 'node:path';
import puppeteer from '/Users/enzo/Code/seb/harness/node/node_modules/puppeteer/lib/esm/puppeteer/puppeteer.js';
import { renderMermaid } from '/Users/enzo/Code/seb/harness/node/node_modules/@mermaid-js/mermaid-cli/src/index.js';
const root = process.argv[2];
const files = [];
(function walk(d){ for (const e of fs.readdirSync(d,{withFileTypes:true})) { const p=path.join(d,e.name); if(e.isDirectory()) walk(p); else if(p.endsWith('.mmd')) files.push(p);} })(root);
files.sort();
let browser = await puppeteer.launch({ args: ['--no-sandbox'], headless: 'shell' });
const opts = { viewport: { width: 800, height: 600, deviceScaleFactor: 1 }, backgroundColor: 'white', mermaidConfig: { theme: 'default' }, svgId: 'my-svg' };
const stats = { ok: 0, unstable: 0, error: 0 }; const log = [];
let n = 0;
async function once(def) { if (++n % 30 === 0) { await browser.close(); browser = await puppeteer.launch({ args: ['--no-sandbox'], headless: 'shell' }); } const { data } = await renderMermaid(browser, def, 'svg', opts); return Buffer.from(data).toString('utf8'); }
for (const f of files) {
  if (fs.existsSync(f.replace(/\.mmd$/, '.svg'))) { stats.ok++; continue; }
  const def = fs.readFileSync(f, 'utf8');
  try {
    const a = await once(def);
    if (/aria-roledescription="error"|Syntax error in text/.test(a)) { stats.error++; log.push(`ERR ${f}`); fs.appendFileSync(path.join(root,'render.log'),`ERR ${f}\n`); continue; }
    const b = await once(def);
    if (a !== b) { stats.unstable++; log.push(`UNSTABLE ${f}`); continue; }
    fs.writeFileSync(f.replace(/\.mmd$/, '.svg'), a); stats.ok++;
  } catch (e) { stats.error++; log.push(`ERR ${f} ${String(e.message).split('\n')[0].slice(0,100)}`); }
}
await browser.close();
fs.writeFileSync(path.join(root, 'render.log'), log.join('\n') + '\n');
console.log(JSON.stringify(stats));
