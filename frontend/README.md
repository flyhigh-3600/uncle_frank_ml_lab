# Frank's Plan Editor (local prototype)

A dependency-light, static UI prototype for browsing local demo plans, editing
text, inspecting a graph, and stepping through a mock terminal session. It does
**not** parse/compile `.ufp`, execute a model, or connect to backend graph APIs.
“Validate wiring” checks only a few local text-shape conditions; it is not a
type checker or graph validator.

## Build and serve

Requires Node.js/npm; no package installation is needed.

```sh
npm run build
```

This writes the static bundle to `frontend/dist`, the directory used by the
backend static fallback. Start the Rust server from `backend/` with `cargo run`
and open <http://127.0.0.1:8787>. You can also serve `dist/` with any static
file server. To edit the source directly without building, open `index.html`.

Draft text is stored in browser local storage. Demo files and session output
remain in this browser; no requests are made by the UI.
