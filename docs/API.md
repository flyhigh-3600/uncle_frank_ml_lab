# Backend API

Existing graph endpoints remain available alongside the UFP text endpoints:

* `POST /api/ufp/parse` — JSON `{ "source": "..." }`; returns parsed declaration summaries, or `422` with diagnostics.
* `POST /api/ufp/check` — JSON `{ "source": "..." }`; returns type-check diagnostics (or syntax diagnostics) in a consistent `{ ok, diagnostics }` response.
* `POST /api/ufp/run` — JSON `{ "source": "...", "inputs": { "name": "text" } }`; runs the supported text-terminal plan and returns outputs/events. Unsupported programs return `422` with a runtime diagnostic.
* `GET /api/ufp/examples/:name` — returns source for a named `.ufp` file located directly under `examples/`. Names with path separators, traversal components, or unsupported characters are rejected.

Diagnostic objects contain `code`, `message`, and a `span` with byte `start`/`end` plus 1-based `line`/`column`. Parse and check use `422 Unprocessable Entity` when diagnostics are present.
