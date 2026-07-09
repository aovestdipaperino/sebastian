# FINDINGS

Small discoveries made along the way that don't fit the CHANGELOG or
`docs/NUANCES.md` (which catalogs Chrome/V8/mermaid rendering behaviors).
These are environment, toolchain, and process findings — the kind of thing
that costs an hour the second time you hit it.

## WASM port (2026-07)

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

## Fonts

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

## Test corpus

- **The gantt corpus is timezone-sensitive**: references were generated in
  Pacific time and only pass under `TZ=America/Los_Angeles` (verified: fails
  in UTC and Europe/Rome, at a byte offset deep in the axis markup). Gantt
  date arithmetic is deliberately naive-local (matching dayjs in the
  rendering browser), so this is inherent, not a bug. CI pins the TZ.

## Toolchain / release process

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
