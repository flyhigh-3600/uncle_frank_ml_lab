# Frank's Plan Editor (local prototype)

A dependency-light local editor slice for browsing the repository's actual
`examples/` files, editing UFP text, inspecting a demo canvas, and stepping
through a mock terminal session. The build copies `../examples/` to
`frontend/dist/examples/`; the browser fetches examples from that local bundle.
UFP edits are stored in this browser's local storage. `.txt` is inert and
read-only.

The UI calls `/api/health`, loads `/api/graph/demo`, and posts that backend demo
graph to `/api/graph/validate`. Those diagnostics apply only to the backend's
demo ML graph, not to the open UFP source or the illustrative editor canvas.
The UFP editor does **not** parse, compile, or type-check UFP. “Validate wiring”
only checks the illustrative local canvas. “Run session” is a timed mock and
does not call a UFP execution endpoint or run a model.

## Build and serve

Requires Node.js/npm; no package installation is needed.

```sh
npm run build
npm test
```

These commands run from `frontend/`. Build writes the static bundle and actual
examples to `frontend/dist`, served by the backend static fallback. Start the
Rust server from the repository root with `cargo run -p frank-backend` and open
<http://127.0.0.1:8787>. `npm test` checks the grammar-shaped fixture assertions
and verifies source examples exist; no parser/type-checker is implied.

Draft text and session output remain in this browser. Local requests go to the
same-origin backend and bundled example files.
# Frontend checks

Run the fast static checks and build with `npm test && npm run build`.
Browser E2E requires Node/npm, Rust/Cargo (to launch `frank-backend`) and
Playwright's Chromium browser:

```sh
npm install
npx playwright install chromium
npm run test:e2e
```

Playwright builds the frontend and starts the backend on `127.0.0.1:8789` by
default. Override with `FRANK_E2E_PORT` if needed. E2E covers the real health
endpoint, editor/source interactions, mock session output, backend diagnostics
(including its explicit fallback), portal layer navigation and inert `.txt`
behavior. The editor currently has no separate terminal input or compiler/API
integration; the source textarea and mock Run Session UI are what is exercised.
