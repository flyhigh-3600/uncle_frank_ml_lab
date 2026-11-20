# Task Ledger

Every unit of work in Uncle Frank's Lab. The narrative/checklist view lives in
[`CHECKLIST.md`](./CHECKLIST.md); this file is the index. Epics E0–E9 are mirrored
as GitHub issues.

- **S** ≈ half a day · **M** ≈ 1–2 days · **L** ≈ 3+ days (for one Frank)
- Status: `todo` · `doing` · `done` · `blocked`
- A task is done only when its acceptance criteria in `CHECKLIST.md` are met,
  tests pass, and the spec is updated if semantics changed.

## Milestones

| # | Name | Contains | Exit condition |
|---|---|---|---|
| M0 | Skeleton | E0, E1 (lex/parse) | `ufp parse` reads the example corpus |
| M1 | Language | E2, E3, E4 | typed IR + interpreter runs examples |
| M2 | Editor | E5 | wire, drill into portals, see T zones |
| M3 | Native | E6 | C/ggml build matches interpreter |
| M4 | Demo | E7 | load a GGUF and generate tokens end to end |
| M5 | Hardening | E8, E9 | conformance corpus, fuzzing, reproducible artifacts, docs |

---

## E0 — Foundations

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-001 | Cargo workspace: `ufp-syntax`, `ufp-types`, `ufp-ir`, `ufp-runtime`, `ufp-codegen`, `ufp-cli`; keep `frank-backend` as demo | M | — | doing |
| UF-002 | CI baseline: fmt, clippy, tests, web build | S | UF-001 | doing |
| UF-003 | Repo hygiene: `.gitignore`, LICENSE decision, CONTRIBUTING, issue/PR templates | S | — | todo |
| UF-004 | Preserve cursed runtime as reference executor + document its JSON API | S | — | doing |
| UF-005 | Decision log (`docs/DECISIONS.md`) for Q1–Q15 | S | — | doing |
| UF-006 | Requirements traceability: checklist, ledger, specs and issues stay in sync | S | — | todo |

## E1 — Syntax

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-101 | Lexer: tokens, spans, comments, literals, annotations | M | UF-001 | todo |
| UF-102 | Parser: plan level (import, portal, text, procedure, node, pipe) | M | UF-101 | todo |
| UF-103 | Parser: statements & expressions (if/elif/else, run-until, structured, matrix/list, index, field) | L | UF-102 | todo |
| UF-104 | Diagnostics: stable spans, error codes, recovery | M | UF-102 | todo |
| UF-105 | Golden parser tests + AST snapshots | M | UF-103 | todo |
| UF-106 | Formatter + source↔IR round-trip tests | M | UF-103 | todo |
| UF-107 | `.txt` attachments: parse, carry, expose as inert notes | S | UF-102 | todo |
| UF-109 | Named matrix/binary declarations: nested rows, byte arrays, lazy binary matrix views | M | UF-102 | todo |
| UF-108 | I/O terminals: `input`/`output` text boxes, trigger/sink lowering | M | UF-102 | doing |

## E2 — Types & constraints

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-201 | Type model: bool, int\<n\>, float\<n\>, bfloat\<16\>, quant\<fmt\>, string, binary, sheet, list, matrix, structured, condition | L | UF-103 | todo |
| UF-202 | Scopes, symbol tables, `.`/`..`/`...` scope walking, pin/param resolution | M | UF-103 | todo |
| UF-203 | Expression typing: indexing, dot access, struct literals, operators | L | UF-201, UF-202 | todo |
| UF-204 | Comparison semantics: element-wise bool matrix/struct, orderability errors | M | UF-203 | todo |
| UF-205 | Pin modes: explicit, inferred (`auto`), inout; fixpoint inference | L | UF-203 | todo |
| UF-206 | Constraint language + deterministic solver (shape/rank/dim/elem) | L | UF-201 | todo |
| UF-207 | Diagnostic catalogue with codes and hints | M | UF-104, UF-203 | todo |

## E3 — IR & lowering

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-301 | IR schema: hierarchical nodes/ports/edges, zones, spans, metadata | M | UF-203 | todo |
| UF-302 | Lower straight-line procedures, pipes, bindings and I/O terminals | M | UF-301 | todo |
| UF-303 | Lower if/elif/else → predicate/branch/merge | L | UF-302 | todo |
| UF-304 | Lower `run … until` → bounded do-until fold (gated feedback, fuel) | L | UF-302 | todo |
| UF-305 | Portal/module resolution, cycle policy, content hashing | L | UF-301 | todo |
| UF-306 | Lower structured/list/sheet values | M | UF-203, UF-302 | todo |
| UF-307 | IR validation: types, fan-in, cycles, zone order | M | UF-301 | todo |
| UF-308 | IR ↔ editor JSON round-trip (positions, nesting, comments) | M | UF-301 | todo |
| UF-309 | Rust interpreter over IR (matrix/list/structured, folds, portals, I/O triggers) | L | UF-302 | todo |
| UF-310 | Trigger protocol: snapshot inputs, fire graph, capture outputs as stable JSON | M | UF-302, UF-309 | todo |
| UF-311 | Execution observation: bounded lifecycle events, coalescing, presentation-only slow mode | M | UF-309 | todo |

## E4 — Time zones

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-401 | Parse `@T(a.b.c)` and legal zone paths | S | UF-103 | todo |
| UF-402 | Zone semantics: nesting, order, concurrency; spec tests | M | UF-401 | todo |
| UF-403 | Auto-inference (ASAP) from data dependencies | M | UF-301, UF-402 | todo |
| UF-404 | Validation: zone inversion, duplicate paths, races | M | UF-403 | todo |
| UF-405 | Editor zone rendering: bands, legend, per-zone status | M | UF-501, UF-404 | todo |
| UF-406 | Strict/relaxed schedule modes in codegen and editor preview | S | UF-402, UF-607 | todo |

## E5 — Editor

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-501 | Web scaffold (Vite/TS), lab theme, palette/canvas/inspector | M | — | todo |
| UF-502 | Node/port rendering with type and shape badges | M | UF-501, UF-308 | todo |
| UF-503 | Semantic drag-drop wiring with refusal reasons | L | UF-502, UF-307 | todo |
| UF-504 | Inspector: pins, types, constraints, params, span, zone | M | UF-502 | todo |
| UF-505 | Portal drill-in: nested tabs, breadcrumbs, signature migration | L | UF-503, UF-305 | todo |
| UF-506 | Pipe editing: reroute, rename, delete | M | UF-503 | todo |
| UF-507 | Undo/redo, multi-select, copy/paste, autosave | L | UF-502 | todo |
| UF-508 | Project workspace: file tree, multiple `.ufp`, inert `.txt` notes | M | UF-501 | todo |
| UF-509 | Compile/run panel: streamed diagnostics, logs, artifacts | M | UF-503, UF-607 | todo |
| UF-510 | Keyboard shortcuts + accessibility pass | S | UF-507 | todo |
| UF-513 | Run session: coarse active-block highlighting + speed control | M | UF-509, UF-311 | todo |
| UF-511 | I/O text boxes: editable input, read-only output, source write-back | M | UF-108, UF-502 | todo |
| UF-512 | Dual graph/text view kept in sync through the shared IR | M | UF-106, UF-308, UF-502 | todo |
| UF-514 | Folder workspace + lazy inspector: nested tabs, compact portals/layers, bounded matrix slices | L | UF-501, UF-505, UF-508 | todo |

## E6 — Compiler (C + ggml)

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-601 | C runtime header: tensor/list/sheet/struct ABI, arena, ownership | L | UF-301 | todo |
| UF-602 | C codegen: scalars, control flow, structured, string/binary | L | UF-601, UF-303 | todo |
| UF-603 | Matrix ops → ggml mapping + partitioner (incl. quantized matmul) | L | UF-601 | todo |
| UF-604 | ggml build integration (vendored by default), deterministic artifacts | M | UF-603 | doing |
| UF-605 | Fold codegen with fuel + predicate | M | UF-602, UF-304 | todo |
| UF-606 | Portal → separate translation units + linker | M | UF-602, UF-305 | todo |
| UF-607 | Compiler driver: clang flags, content-addressed cache, manifest | M | UF-602 | todo |
| UF-608 | Differential tests: interpreter vs C/ggml | L | UF-607, UF-309 | todo |
| UF-609 | Sandbox + capability policy for builtins and generated code | M | UF-607 | todo |
| UF-610 | Text terminal entrypoint: input parameters, output serialisation, stable exit codes | M | UF-602, UF-607 | todo |
| UF-611 | Reproducible build: content-addressed artifacts, deterministic toolchain flags | M | UF-607 | todo |

## E7 — ML utilities & examples

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-701 | Builtin registry with typed pins and capability classes | M | UF-203 | todo |
| UF-702 | GGUF import → nested `structured` (metadata + matrix substructures), specialised types | L | UF-701, UF-601 | todo |
| UF-708 | Local data loaders: binary/byte arrays, JSON, CSV; workspace sandbox and typed results | M | UF-201, UF-701 | todo |
| UF-709 | GGUF → UFP exporter: layer plans, binary views, portal-root model | L | UF-702, UF-708, UF-305 | todo |
| UF-703 | `condition` combinators and capture checks | S | UF-204 | todo |
| UF-704 | Example corpus: one `.ufp` per feature | M | UF-103 | todo |
| UF-705 | End-to-end demo: load GGUF, run generation fold, inspect tokens | L | UF-604, UF-702, UF-309 | todo |
| UF-706 | Text-terminal examples: prompt → reply with and without GGUF | S | UF-108, UF-309 | todo |
| UF-707 | Precision/quantization example: `float<16>`, `bfloat<16>`, `quant<q4_0>` | S | UF-108, UF-201, UF-702 | todo |

## E8 — QA

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-801 | Cross-crate unit + golden test suites | M | UF-105, UF-201 | todo |
| UF-802 | Constraint solver property tests | M | UF-206 | todo |
| UF-803 | Lowering snapshot tests (one per rule) | M | UF-303 | todo |
| UF-804 | Parser fuzzing (cargo-fuzz) | M | UF-104 | todo |
| UF-805 | CI matrix + release checklist | S | UF-002 | todo |
| UF-806 | Editor end-to-end tests: drag-drop, portal drill-in, text boxes, zones | M | UF-503, UF-505, UF-511 | todo |
| UF-807 | Differential text-terminal tests: CLI, interpreter, native entrypoint | S | UF-608, UF-610 | todo |

## E9 — Docs & community

| ID | Task | Size | Depends on | Status |
|---|---|---|---|---|
| UF-901 | Language reference (mdBook) generated alongside the spec | M | UF-207 | todo |
| UF-902 | Tutorial: first plan → compile → run | M | UF-309, UF-501 | todo |
| UF-903 | Architecture & compiler-internals docs | M | UF-301, UF-607 | todo |
| UF-904 | Contribution guide + roadmap sync | S | UF-003 | todo |
| UF-905 | Operations guide: backend, CLI, editor, native artifacts | S | UF-004, UF-607 | todo |

---

## Working an item

1. Pick a task whose dependencies are `done`.
2. Open/claim its GitHub issue; set status `doing` here in the same PR.
3. Keep the change scoped to the task; update `docs/SPEC.md` if semantics move.
4. Acceptance criteria from `CHECKLIST.md` are the review checklist.
5. Mark `done` only with tests; otherwise leave `doing` and note the blocker.
