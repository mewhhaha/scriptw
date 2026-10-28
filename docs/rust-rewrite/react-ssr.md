# React SSR through Wasm-AOT — 2026-10-20

**Renamed release recheck:** source `c5053c9d4` also passes these same three
synchronous HTML comparisons with the ordinary opt3/default-feature `scriptw`
CLI. Its dependency graph contains no product oracle. The identical bundle
hash, literal Node/Wasm outputs and Cargo identity are preserved in
[the new release evidence](v8-baseline-20261020.json). The public browser entry
was not retested; its original failure remains below. This is still a focused
synchronous example, not full React compatibility or release acceptance.


The recorded run predates the [scriptw rename](scriptw-identity-migration.md).
Its receipts retain the actual earlier executable identity; reproduction
commands below use the current project spelling.

**React 19.3.1 synchronous server rendering works in this probe.** The
unmodified ReactDOM synchronous production renderer ran through Scriptw's
Wasm-AOT path and produced exactly the same three HTML records as Node.
The public `react-dom/server.browser` entry failed; both results are retained
in the [source-bound evidence](react-ssr-20261020.json).

This is a compatibility example at compiler source `cbf7cc71f`, using the
existing debug CLI. It adds no compiler changes and closes no conformance or
release gate. Wasm remains the sole product compilation target.

## What ran

The probe installs pinned official npm packages inside ignored `target/`:
React **19.3.1**, ReactDOM **19.3.1** and esbuild **0.28.2**. esbuild bundles
the actual upstream JavaScript into a production IIFE Script. Both variants
use the same entry, React components and production environment definition.
No DOM, stream, encoding or JavaScript semantic polyfills are supplied.

| Entry layout | Actual Scriptw result |
| --- | --- |
| Public `react-dom/server.browser` | Exit 1: `ReferenceError: wasm-aot completion: object: unbound identifier`; no HTML output |
| Same import aliased to upstream `react-dom/cjs/react-dom-server-legacy.browser.production.js` | Exit 0; all three HTML records match Node byte for byte; terminal outcome identifies `WasmAot` |

The alias selects ReactDOM's unmodified synchronous renderer. It is an
internal package path pinned to the tested release, so this integration
needs rechecking when ReactDOM changes. It does not replace React rendering
with a shim or supply previously rendered HTML.

The three cases exercise:

- A plain element and an `id` attribute.
- A function component, lazy `useState`, list rendering and escaping of text.
- Context/`useContext`, a Fragment, styles, a boolean attribute and input
  default-value escaping.

For example, the first output is `<h1 id="hello">Hello Wasm</h1>`. The full
input, exact three Node/Scriptw outputs, diffs, package integrity lock, upstream
sources/licenses and literal commands are preserved in the evidence.

The debug CLI was built with `spec-exec-oracle`, as recorded in the existing
[capability inventory](current-inventory.md). The successful execution
explicitly reports `backend_used: WasmAot`; no interpreter fallback was used.

## Public entry limitation

The public browser entry eagerly loads both the synchronous legacy renderer
and the modern streaming renderer. Source inspection finds module-level
`MessageChannel` and `TextEncoder` construction in the modern renderer.
Scriptw does not provide those browser APIs. The observed exception names only
an unbound identifier; this probe does **not** establish which missing global
is encountered first.

The first Node reference driver rendered all three records but remained alive
with the public entry's idle ports and reached its 60-second process guard.
No Scriptw execution had started. A fresh driver reused byte-identical bundles
and explicitly exited Node after synchronous rendering returned. That driver
completed both Node references and both Scriptw executions, retaining the failed
public entry. The initial reference-driver failure remains in the evidence.

Streaming SSR, Suspense streaming, hydration, DOM/browser operation, Node API
compatibility and arbitrary React applications are unverified. These results
do not establish full React support or a production server deployment.

## Reproduce the synchronous case

Build a source-bound `target/debug/scriptw` as described in the
[current inventory](current-inventory.md), then start from the repository root.
The following commands create an isolated example; they do not change the
repository's package manifests.

```sh
SCRIPTW_BIN="$(pwd)/target/debug/scriptw"
mkdir -p target/react-ssr-example
cd target/react-ssr-example
npm install --ignore-scripts --no-audit --no-fund --save-exact \
  --registry=https://registry.npmjs.org \
  react@19.3.1 react-dom@19.3.1 esbuild@0.28.2
cat > entry.js <<'JS'
import React from 'react';
import {renderToString} from 'react-dom/server.browser';
const e = React.createElement;
function App({name, items}) {
  const [count] = React.useState(() => items.length);
  return e('section', {className:'greeting', 'data-count':count},
    e('h1', null, 'Hello ', name),
    e('ul', null, items.map((item, i) => e('li', {key:i}, item))));
}
const Theme = React.createContext('light');
function Themed() { return e('span', {className:React.useContext(Theme), style:{marginTop:2,color:'red'}}, 'context'); }
const cases = [
  ['element', e('h1', {id:'hello'}, 'Hello Wasm')],
  ['component-state-list-escaping', e(App, {name:'<Wasm>', items:['<one>', 'two & three']})],
  ['context-style-fragment', e(React.Fragment, null, e(Theme.Provider, {value:'dark'}, e(Themed)), e('input', {disabled:true, defaultValue:'a"b'}))]
];
for (const [name, tree] of cases) print(JSON.stringify({react:React.version, name, html:renderToString(tree)}));
JS
./node_modules/.bin/esbuild entry.js --bundle --platform=browser \
  --format=iife --target=esnext --minify --legal-comments=inline \
  '--define:process.env.NODE_ENV="production"' \
  --alias:react-dom/server.browser=./node_modules/react-dom/cjs/react-dom-server-legacy.browser.production.js \
  --outfile=ssr.js
# The reference only adapts Scriptw's print output extension to Node.
node -e 'globalThis.print = console.log; require(process.argv[1]); process.exit(0);' \
  ./ssr.js
cd ../..
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  "$SCRIPTW_BIN" --jobs 4 run target/react-ssr-example/ssr.js
SCRIPTW_WASM_DUMP=target/react-ssr-example/ssr.wasm \
  python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  "$SCRIPTW_BIN" --jobs 4 build wasm target/react-ssr-example/ssr.js
```

Omitting the alias reproduces the public entry layout. The cloud wrapper uses
the cloud machine's inherited resource limits. The recorded runs used four
compiler jobs, separate fresh caches per variant and a 180-second outer guard;
no existing suite worker deadline or cache capacity was changed. Exact commands
and original bundle bytes are retained for reproduction independent of paths.

## Emitted Wasm

`build wasm` also successfully emitted the synchronous SSR program and its
matching shared runtime sidecar, with both Wasm v1 headers verified:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Program | 12,532,009 | `5292ca4a7b250c847f81e304c00994959866c3d7901a60d37ea500f9035020a7` |
| Runtime sidecar | 40,136,141 | `d734bfa71274013c15bb9b0b6ff0b4910b1dc103bd98b1a8570312c0346fe56d` |

The binary pair is retained under ignored
`target/react-ssr-compatibility-20261020-run2/`; its bytes are not embedded in
Git. The evidence contains the emission receipt and hashes. These artifacts
require Scriptw's runtime imports and host adapters on its experimental Wasmtime
target. Generic browser or WASI loading was not tested. The emission check is
separate from the successful `run` execution; no separate external loader was
used to execute the dumped pair.

Cold debug observations were about 20.3 seconds for the passing run and 7.9
seconds for emission with its existing cache. These are diagnostic wall times,
not a calibrated benchmark or a Node performance comparison. No Cargo rebuild
or previously completed suite body was needed for this probe. The broader
[unfinished verification and release work](current-inventory.md#next-work)
is unchanged.
