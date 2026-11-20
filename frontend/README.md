# Frank's Plan Editor (local prototype)

A dependency-light local editor slice for browsing the repository's actual
`examples/` files, editing UFP text, inspecting a demo canvas, and stepping
through a mock terminal session. The build copies `../examples/` to
`frontend/dist/examples/`; the browser fetches examples from that local bundle.
UFP edits are stored in this browser's local storage. `.txt` is inert and
read-only.

The UI calls `/api/health`, loads `/api/graph/demo`, and posts that backend demo
graph to `/api/graph/validate`. It also attempts the optional UFP endpoints
`POST /api/ufp/parse`, `/api/ufp/check`, and `/api/ufp/run` with `{source}`.
Run adds `inputs: {prompt: <editable session value>}`. Diagnostics, output maps,
and emitted events are shown in the terminal when returned. If the UFP API is
not available, local canvas checks and a clearly labeled canvas mock are used;
the mock is not a UFP execution. The illustrative graph remains independent of
the open text source and is not rewritten from API parse results.

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
and verifies source examples exist; frontend-side fallback does not imply a
parser/type-checker is available.

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
