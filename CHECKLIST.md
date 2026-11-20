# Uncle Frank's Lab — Master Checklist

The executable view of the project. Task IDs match [`TASKS.md`](./TASKS.md) and
the GitHub epics. Specs: [`docs/UFP-SPEC.md`](./docs/UFP-SPEC.md) (normative) ·
[`docs/SPEC.md`](./docs/SPEC.md) (design) ·
[`docs/grammar.ebnf`](./docs/grammar.ebnf) (syntax) ·
[`docs/DECISIONS.md`](./docs/DECISIONS.md) (rulings).

**Current focus:** M0 — workspace, lexer, plan parser, text terminals.
`frank-backend` builds and trains; the `.ufp` toolchain is being cut now.

Legend: `[ ]` not done · `[~]` in progress · `[x]` done · `[!]` blocked.
A task is done only when its boxes are ticked, tests pass, the spec/grammar are
updated, and `TASKS.md` says `done`.

---

## Original brief → task map

Every requirement from the original brief has an owner. If a box here is empty,
the project is not done, no matter how green the rest looks.

| # | Requirement | Epics | Milestone |
|---|---|---|---|
| R1 | Web graph editor; `.ufp` plans + inert `.txt`; HDL-like nodes, pipes, portals, pins, types, condition, `run…until`, lists | E1, E2, E5 | M0–M2 |
| R1a | `input`/`output` procedures opened as text boxes, spawned/focused by Run, used as runtime source/trigger | E1, E3, E5 | M0–M2 |
| R2 | Formal, usable grammar — not a PhD thesis | E1 | M0 |
| R3 | Formal source → graph conversion; portals inspectable/editable by click | E3, E5 | M1–M2 |
| R4 | `T(l.m.n…)` time markers with ordered/concurrent zones | E4 | M1–M2 |
| R5 | Draw the graph and render each time as a zone | E4, E5 | M2 |
| R6 | Compile to C; matrix-only procedures become ggml execution | E6 | M3 |
| R7 | Drag-and-drop wiring with correct input/output semantics | E5 | M2 |

---

## Milestones

| # | Name | Exit condition |
|---|---|---|
| M0 | Skeleton | `cargo test --workspace` passes; `ufp parse` reads the example corpus, including text terminals |
| M1 | Language | typed IR + interpreter runs examples, folds, I/O triggers, and portals |
| M2 | Editor | add `.ufp`/`.txt`, wire nodes, edit text boxes, drill into portals, see zones |
| M3 | Native | C/ggml build matches the interpreter; matrix-only subgraphs run in ggml |
| M4 | Demo | load a GGUF, generate tokens, show text input → text output |
| M5 | Hardening | conformance corpus, fuzzing, reproducible artifacts, docs |

---

## Start here (M0)

- [~] **UF-001** Cargo workspace
  - [x] workspace manifests for `ufp-syntax`, `ufp-types`, `ufp-ir`, `ufp-runtime`, `ufp-codegen`, `ufp-cli`
  - [x] `frank-backend` stays a workspace member
  - [x] one root `Cargo.lock`; nested lock removed
  - [ ] `cargo test --workspace` passes from a clean checkout
  - [ ] `cargo fmt --check` + `cargo clippy --workspace -- -D warnings`
- [~] **UF-004** Reference executor documented
  - [x] `docs/RUNTIME.md`: endpoints, graph JSON, curses, measured effect
  - [ ] add the `ufp` CLI/text-terminal path once UF-108 lands
- [~] **UF-005** Decision log
  - [x] D-001…D-012 recorded
  - [x] Q1, Q2, Q7, Q14 and precision resolved
  - [ ] Q3–Q6, Q8–Q13, Q15 resolved or explicitly deferred
- [~] **UF-002** CI baseline — fmt + clippy + tests + web build on push
- [ ] **UF-003** repo hygiene — `.gitignore`, license decision, CONTRIBUTING, issue/PR templates
- [ ] **UF-101** Lexer — every token, span, comment, literal and annotation
- [ ] **UF-102** Plan-level parser — every top-level declaration, including text terminals
- [ ] **UF-103** Statement/expression parser — `if`, folds, structures, matrices, scopes, port chains
- [ ] **UF-104** Diagnostics and recovery — stable codes, spans, hints, no panics
- [~] **UF-108** I/O terminals
  - [x] syntax and semantics written (`input` trigger, `output` sink, text only)
  - [x] example `examples/text_io.ufp`
  - [ ] parser + AST
  - [ ] IR lowering to 0-in/1-out and 1-in/0-out text procedures
  - [ ] runtime trigger + output capture
  - [ ] editor text boxes
  - [ ] Run session: coarse active-block highlighting + optional presentation-only slow mode

---

## E0 — Foundations

- [~] **UF-001** workspace restructure — *see Start here*
- [~] **UF-002** CI baseline
  - [ ] `cargo fmt --check`
  - [ ] `cargo clippy --workspace --all-targets -- -D warnings`
  - [ ] `cargo test --workspace`
  - [ ] web build job (once UF-501 lands)
- [ ] **UF-003** repo hygiene
  - [ ] license decision recorded (currently undecided, all rights reserved)
  - [ ] CONTRIBUTING, issue/PR templates
  - [ ] no build artifacts tracked
- [~] **UF-004** reference executor preserved + documented
- [~] **UF-005** decision log current
  - [ ] every `Qn` has a `D-###` entry or a dated deferral
  - [x] every resolved `D-###` is reflected in the normative spec
- [ ] **UF-006** traceability kept honest — this file, `TASKS.md`, issues and specs agree after every semantic change

---

## E1 — Syntax

### UF-101 — Lexer

- [ ] UTF-8 source; BOM tolerated; LF/CRLF
- [ ] line comments `//`, block comments `/* … */`; unterminated comment is `UFP1002`
- [ ] identifiers and all reserved keywords, including `input`/`output`
- [ ] integer, hex integer, float, exponent, string escapes
- [ ] raw text block `"""…"""` with preserved newlines; unterminated is `UFP1006`
- [ ] punctuation: `( ) { } [ ] , ; : . -> @`
- [ ] operators: `+ - * / % .* == != <= >= < > = & | ^ ~ << >>`
- [ ] `.*` is one token; `->` is one token; dot runs remain individual `DOT`s
- [ ] zone-aware `@T(0.1.2)` lexes as `INT DOT INT DOT INT`
- [ ] every token carries byte span + line/column
- [ ] error tokens never panic; fuzz-safe

### UF-102 — Plan-level parser

- [ ] `import qualified [from "…"] [as name];`
- [ ] `portal name = "…" [as Name];`
- [ ] `text name = "…";` — inert `.txt` attachment
- [ ] `procedure name(pins) { … }` and `procedure name(pins) graph { … }`
- [ ] pins: direction, static type, `auto`, `with` constraints, optional name
- [ ] `node id : target(bindings);`
- [ ] `pipe [id:] a.x -> b.y;`
- [ ] annotations `@name(args)`, `@T(l.m.n)`
- [ ] `input name = "text";` and `output name;` (D-012)
- [ ] top-level recovery; a bad declaration cannot eat the rest of the file
- [ ] parses `examples/*.ufp`

### UF-103 — Statement and expression parser

- [ ] `let`, assignment, `return`, expression statements
- [ ] `if / elif / else`
- [ ] `seed run(body until (condition))`
- [ ] `structured { … }` with typed and shorthand fields
- [ ] list and matrix literals; matrix rank/extents/`of` element type
- [ ] index `m[i,j]`, field `x.f`, call `f(a)`, input port map `f.(a)`
- [ ] port chain `f.(a) g(b)`
- [ ] scope paths `.f`, `..f`, `...f`
- [ ] precedence and non-chaining comparisons
- [ ] AST preserves a span on every node

### UF-104 — Diagnostics and recovery

- [ ] `UFP1xxx` lexical and `UFP2xxx` syntax codes
- [ ] span, severity, message, optional hint in the stable JSON shape
- [ ] deterministic source ordering
- [ ] parser continues after a bad declaration
- [ ] no panic on arbitrary input

### UF-105 — Golden tests

- [ ] one corpus file per language feature
- [ ] accepted files produce zero errors
- [ ] rejected files assert exact code + span
- [ ] AST snapshot diffs reviewed, never blind-accepted

### UF-106 — Formatter and round-trip

- [ ] `format(parse(src))` is idempotent
- [ ] comments and annotations survive formatting
- [ ] graph editor → IR → text → IR is structurally lossless
- [ ] canvas positions/comments live in side tables, not in executable IR

### UF-107 — `.txt` attachments

- [ ] parsed into the project manifest, never lexed as `.ufp`
- [ ] editor shows `.txt` as a notes pane only
- [ ] cannot be imported, portalled, compiled, or type-checked

### UF-108 — Input/output text terminals

- [ ] `input prompt = "…";` parses; literal required; `""` legal
- [ ] `output reply;` parses; exactly one binding may arrive on `reply.text`
- [ ] lowering: input = 0-in / 1-out `string`; output = 1-in / 0-out `string`
- [ ] input is the run **source and trigger**; all inputs snapshot and fire
- [ ] output is the run **sink**; every output must be wired or `UFP3404`
- [ ] terminal names share the plan namespace; duplicates are `UFP3403`
- [ ] pipes touching a terminal must use the `text` pin; otherwise `UFP3405`
- [ ] input is text-only in this version; no file/image/audio/model sources
- [ ] editor can open input as editable text box and output as read-only box
- [ ] box edits rewrite the source literal, not hidden UI state
- [ ] CLI: `ufp run plan.ufp [--set prompt=text] [--json]`
- [ ] example `examples/text_io.ufp` runs

---

## E2 — Types and constraints

- [ ] **UF-201** type model — all §4.1 types, interning, canonical printing
  - [ ] `bool`
  - [ ] `int`, `int<8|16|32|64>`
  - [ ] `float`, `float<8|16|32|64>`; `float<8, e4m3|e5m2>`
  - [ ] `bfloat<16>`; `double` = `float<64>`
  - [ ] `quant<fmt>` matrix-only; ggml format registry; block-size checks
  - [ ] `string`, `binary`
  - [ ] `list`, `list<T>` (mutable, heterogeneous unless fixed)
  - [ ] `sheet` (provisional Q3)
  - [ ] `matrix<r>[d…] of T`; `<r>` = rank, `of` = element (D-006)
  - [ ] `structured { … }`
  - [ ] `condition`; `auto`
  - [ ] no new type constructors for GGUF or I/O terminals
- [ ] **UF-202** scopes and symbols
  - [ ] `.` self, `..` parent, `...` grandparent (D-011)
  - [ ] procedure body, structured literal and fold body each open a scope
  - [ ] outer reads allowed; writes through `..`/`...` are `UFP3302`
  - [ ] static resolution: no runtime lookup, no dynamic scoping
  - [ ] shadowing rules documented; parameter shadow warning
- [ ] **UF-203** expression typing
  - [ ] index arity equals rank; static bounds where provable, runtime otherwise
  - [ ] dot access on `structured`/`sheet`; unknown field error
  - [ ] `*` = matmul (`ggml_mul_mat`), `.*` = Hadamard (`ggml_mul`)
  - [ ] `/` = `ggml_div`; `+`/`-` = `ggml_add`/`ggml_sub`
  - [ ] shape mismatch prints both shapes
  - [ ] quantized matmul with f32 accumulation; explicit `dequantize` for element work
  - [ ] structured/list element-wise operators
- [ ] **UF-204** comparisons
  - [ ] matrix comparison → bool matrix of the same shape
  - [ ] `list == list` → `list<bool>`; ragged lists error
  - [ ] structured comparison keeps labels, bool-ifies fields
  - [ ] ordering requires every field orderable; `string`/`binary` equality only (Q5)
  - [ ] reductions only: `all`, `any`, `sum`, `mean`, `count`
- [ ] **UF-205** pins
  - [ ] explicit static pins
  - [ ] inferred `auto` pins to a least fixed point
  - [ ] `inout` pins
  - [ ] I/O terminals are fixed `string` pins
  - [ ] wire-time re-check against the same checker
- [ ] **UF-206** constraint solver
  - [ ] `with a.shape = b.shape` across pins, pipes, terminals
  - [ ] rank/dims/elem/length attributes
  - [ ] deterministic minimal contradiction
  - [ ] property test: never accepts an ill-typed system
- [ ] **UF-207** diagnostic catalogue — codes, hints, stability policy

---

## E3 — IR and lowering

- [ ] **UF-301** IR schema — hierarchical nodes/ports/edges, zones, spans, stable ids, side metadata
- [ ] **UF-302** straight-line procedures, pipes, bindings, I/O terminals
  - [ ] input terminal → source node (trigger)
  - [ ] output terminal → sink node (captured result)
  - [ ] pipe fan-out allowed; fan-in rejected
- [ ] **UF-303** `if/elif/else` → predicate, branch regions, merge
- [ ] **UF-304** `run…until` → fold node (D-010)
  - [ ] body runs at least once
  - [ ] feedback edge gated on "condition unmet"
  - [ ] fuel default 10 000; `@fuel` override
  - [ ] provably unbounded fold → compile error
- [ ] **UF-305** portals
  - [ ] signature from target export
  - [ ] `main` default; `as` selects (Q8)
  - [ ] portal cycles detected with full path
  - [ ] cache key = target hash + compiler version + flags
- [ ] **UF-306** structured/list/sheet lowering
  - [ ] `make_struct` + projections
  - [ ] constant index sees specialised type
  - [ ] dynamic index limited to common fields
- [ ] **UF-307** IR validation — types, fan-in, cycles, zone order, fold fuel
- [ ] **UF-308** editor JSON round-trip — positions, nesting, comments, text boxes, structural equality
- [ ] **UF-309** interpreter
  - [ ] runs every example plan
  - [ ] matrix/list/structured values
  - [ ] folds, portals, conditions
  - [ ] inputs trigger, outputs capture
  - [ ] deterministic; tolerances documented
- [ ] **UF-310** trigger protocol
  - [ ] snapshot all input boxes
  - [ ] fire the graph from every input
  - [ ] capture named outputs + trace as stable JSON
  - [ ] missing input/output is an error, never empty-string fallback
- [ ] **UF-311** execution observation protocol (D-013)
  - [ ] bounded lifecycle events: started/finished node, output update, run finish/fail
  - [ ] normal mode is coarse and low-overhead; no tensor copies or render blocking
  - [ ] event coalescing/dropped count instead of unbounded buffering
  - [ ] speed delay occurs only between events; never changes result/schedule/fuel

---

## E4 — Time zones

- [ ] **UF-401** parse `@T(l.m.n)`; reject negatives/empty segments
- [ ] **UF-402** semantics: lexicographic paths, parent gates children, siblings concurrent candidates
- [ ] **UF-403** ASAP inference for un-annotated nodes
- [ ] **UF-404** validation: zone inversion `UFP4202`, duplicate paths, `@concurrent` races
- [ ] **UF-405** editor rendering: bands, legend, per-zone status, collapse
- [ ] **UF-406** strict/relaxed schedule modes in codegen (`--schedule=strict|relaxed`)

---

## E5 — Web editor

- [ ] **UF-501** Vite/TS scaffold, lab theme, three-pane layout
  - [ ] file tree with `.ufp` and `.txt`
  - [ ] create/open/save `.ufp` plans
  - [ ] `.txt` opens in notes pane only
- [ ] **UF-502** node/port rendering
  - [ ] inputs on the left, outputs on the right (Verilog semantics)
  - [ ] type and shape badges
  - [ ] deterministic port geometry
- [ ] **UF-503** drag-and-drop wiring
  - [ ] drag from palette creates a node
  - [ ] drag from output pin to input pin creates a pipe
  - [ ] accepted only if the checker accepts; refusal shows code + reason
  - [ ] one incoming pipe per input; fan-out allowed
  - [ ] cycles refused except a fold's gated self-edge
  - [ ] named/normalised `node.port` form is what is stored
  - [ ] no silent coercion, ever
- [ ] **UF-504** inspector — pins, types, constraints, params, source span, zone
- [ ] **UF-505** portal drill-in
  - [ ] double-click opens target plan in nested tab
  - [ ] breadcrumb back to caller
  - [ ] target signature change triggers caller migration diagnostics
- [ ] **UF-506** pipe editing — reroute, rename, delete, preserve fan-out
- [ ] **UF-507** undo/redo, multi-select, copy/paste, autosave
- [ ] **UF-508** project workspace — file tree, multiple `.ufp`, inert `.txt` notes
- [ ] **UF-509** compile/run panel
  - [ ] streamed diagnostics and build log
  - [ ] Run creates/focuses a terminal session with editable input and read-only output boxes
  - [ ] inputs are snapshotted; transient session edits do not silently edit source
  - [ ] outputs shown in read-only text boxes
  - [ ] artifact links
- [ ] **UF-513** live execution highlighting and speed control (D-013)
  - [ ] highlight current node/block from coarse runtime events
  - [ ] default mode stays fast: bounded/coalesced events, no per-tensor display copies
  - [ ] slider delays between events only; 0 ms is normal speed
  - [ ] optional later step mode uses the same coarse boundaries
- [ ] **UF-510** keyboard shortcuts + accessibility pass
- [ ] **UF-511** I/O terminal text boxes
  - [ ] double-click input → editable multiline text box
  - [ ] double-click output → read-only result box
  - [ ] edits rewrite the source literal and mark the plan dirty
  - [ ] empty input is legal; unwired output is visibly invalid
- [ ] **UF-512** dual view — graph canvas and text view stay in sync through the IR
  - [ ] switching views does not reorder or lose pipes
  - [ ] comments/positions survive
- [ ] **UF-514** folder workspace and lazy inspection (D-014)
  - [ ] open a single folder as a UFP workspace; file tree distinguishes `.ufp`, `.txt`, data assets
  - [ ] compact portals/procedures/structures/folds expand only by explicit inspect action
  - [ ] nested tabs + breadcrumbs preserve the compact parent canvas
  - [ ] model root composes individually inspectable layer `.ufp` plans
  - [ ] matrix inspector shows metadata first; only explicit bounded slices materialise

---

## E6 — Compiler: C + ggml

- [ ] **UF-601** C runtime ABI
  - [ ] `ufp_value { ptr, len, shape, elem, flags }`
  - [ ] arena allocation per invocation
  - [ ] explicit `owned`/`borrowed`
  - [ ] length-prefixed strings/binary
  - [ ] struct field order + integer widths exactly mapped
- [ ] **UF-602** C codegen — scalars, control flow, structured, string/binary, lists
- [ ] **UF-603** matrix partitioner + ggml mapping
  - [ ] `a * b` → `ggml_mul_mat` with source-row-major transposes inserted
  - [ ] `a .* b` → `ggml_mul`; `/` → `ggml_div`; `+`/`-` → `ggml_add`/`sub`
  - [ ] transpose, reshape, softmax, layer_norm, reductions
  - [ ] quantized matmul paths (`q4_0`, `q4_K`, `q5_K`, `q6_K`, `q8_0`, …)
  - [ ] matrix-only procedures become a ggml graph; mixed plans fall back to C
- [ ] **UF-604** ggml build integration — vendored default, deterministic artifacts
- [ ] **UF-605** fold codegen — do-until loop, fuel counter, predicate, no per-iteration allocation
- [ ] **UF-606** portal translation units + linker + generated headers
- [ ] **UF-607** driver — clang flags, content-addressed cache, JSON manifest, schedule modes
- [ ] **UF-608** differential tests — interpreter vs C/ggml within documented tolerances
- [ ] **UF-609** sandbox — closed builtin registry, capability grants, no raw C/asm/fs escape
- [ ] **UF-610** text terminal entrypoint
  - [ ] input boxes become invocation parameters (or generated stdin wrapper)
  - [ ] output boxes are printed/serialised in declaration order
  - [ ] exit code and diagnostics are stable
- [ ] **UF-611** reproducible build — same plan + seed + toolchain ⇒ same artifact hash

---

## E7 — ML utilities and examples

- [ ] **UF-701** builtin registry — typed pins, effect class (`pure`/`alloc`/`io`/`model`), determinism
- [ ] **UF-702** `import_gguf`
  - [ ] nested `structured` (metadata + `matrices` list), no model type (D-011)
  - [ ] per-file type specialisation (`name`, `shape`, `dtype`, `data`)
  - [ ] constant index sees concrete matrix; dynamic index cannot touch `.data`
  - [ ] dtype mapping incl. quant formats
- [ ] **UF-703** `condition` combinators + capture checks
- [ ] **UF-704** example corpus — one plan per feature, all parsed by CI
- [ ] **UF-705** end-to-end — GGUF → generation fold → decoded text
- [ ] **UF-706** text terminal example — input prompt → output reply, with and without GGUF
- [ ] **UF-707** precision/quantization example — `float<16>`, `bfloat<16>`, `quant<q4_0>`
- [ ] **UF-708** local data loaders
  - [ ] `load_bytes`, byte arrays and binary matrix views
  - [ ] `load_json` → nested `structured`/`list`; `load_csv` → `sheet`
  - [ ] project-folder sandbox; paths cannot escape the workspace
- [ ] **UF-709** GGUF → UFP exporter
  - [ ] one layer `.ufp` or grouped block plan per exported layer
  - [ ] root plan composes layers through portals
  - [ ] tensors use binary matrix views by default; inline only when requested

---

## E8 — QA

- [ ] **UF-801** cross-crate unit + golden suites
- [ ] **UF-802** constraint-solver property tests
- [ ] **UF-803** lowering snapshots — one per rule (I/O, pipe chain, fold, if, structured, scope)
- [ ] **UF-804** parser fuzzing (cargo-fuzz)
- [ ] **UF-805** CI matrix + release checklist
- [ ] **UF-806** editor end-to-end tests — drag-drop, portal drill-in, text boxes, zones
- [ ] **UF-807** differential test include text terminal trigger/output path

---

## E9 — Docs and community

- [ ] **UF-901** language reference (mdBook) generated alongside `UFP-SPEC.md`
- [ ] **UF-902** tutorial: first plan → wire → run → compile
- [ ] **UF-903** architecture and compiler-internals docs
- [ ] **UF-904** contribution guide + roadmap sync
- [ ] **UF-905** operations guide — running the backend, CLI, editor, and native artifacts

---

## Acceptance — original brief, restated

Tick these before calling the ecosystem usable. Each line names the owning task.

### R1 — Editor, two file types, language

- [ ] Open a project in the browser and create/save `.ufp` plans (UF-501, UF-508)
- [ ] `.txt` files are inert notes and never compile (UF-107)
- [ ] Nodes are functions with typed input/output/inout pins (UF-102, UF-205)
- [ ] Pipes are typed data paths; fan-out yes, fan-in no (UF-302, UF-503)
- [ ] `Portal` loads another `.ufp` like a function library (UF-305)
- [ ] `bool`, `int<n>`, `float<n>`, `bfloat<16>`, `quant<fmt>`, `double`, `string`, `binary` (UF-201)
- [ ] `list` is mutable/heterogeneous; `list<T>` fixes the element (UF-201)
- [ ] `sheet` is a 2-D typed-column table (UF-201, Q3)
- [ ] `matrix<r>[d…]` with indexing and `of` element type (UF-201, UF-203)
- [ ] `structured{…}` with dot access and labelled bool-ised comparison (UF-201, UF-204)
- [ ] `if/elif/else` and `condition` (UF-103, UF-203)
- [ ] no loops, only `run…until` do-until folds (UF-304)
- [ ] sizes floats/quantization for real LLM weights (UF-201, UF-603)
- [ ] `.` self, `..` parent, `...` grandparent scope walking (UF-202)
- [ ] `input`/`output` text terminals as source/trigger and sink (UF-108, UF-511)

### R2 — Formal grammar

- [ ] `docs/grammar.ebnf` covers every accepted form, including text terminals and raw text blocks (UF-101, UF-102)
- [ ] grammar is small enough to read; ambiguities are documented and resolved (UF-102)
- [ ] parser accepts the whole corpus; rejects error corpus with exact spans (UF-105)

### R3 — Formal graph conversion and portal inspection

- [ ] source → AST → typed AST → IR is specified and implemented (UF-301, UF-302)
- [ ] every lowering rule has a snapshot test (UF-803)
- [ ] clicking a portal opens the target plan in a nested tab (UF-505)
- [ ] target edits re-check callers; cycles are diagnosed (UF-305)

### R4 — Time-zone markers

- [ ] `@T(0)`, `@T(0.1)`, `@T(0.1.2)` parse and validate (UF-401, UF-404)
- [ ] parents gate children; siblings are concurrent candidates (UF-402)
- [ ] un-annotated nodes get an ASAP zone (UF-403)

### R5 — Draw graph and zones

- [ ] canvas draws nodes/ports/pipes (UF-502)
- [ ] zones render as nested bands with legend and status (UF-405)
- [ ] fold self-edge is drawn as a gated `until` edge (UF-304, UF-502)

### R6 — Compile to C and ggml

- [ ] matrix-only procedures lower to ggml graphs (UF-603)
- [ ] `*` is matmul, `.*` is element-wise, add/sub/div map to ggml (UF-603)
- [ ] mixed scalar/control-flow plans emit C and link with ggml (UF-602, UF-604)
- [ ] portals compile to separate translation units (UF-606)
- [ ] interpreter and native output agree within tolerance (UF-608)

### R7 — Drag-and-drop with semantics

- [ ] palette → canvas node creation (UF-503)
- [ ] output pin → input pin wiring, with refusal reasons (UF-503)
- [ ] inputs enter from the left; outputs exit right (UF-502, D-008)
- [ ] named form `g(in0 = …)` is authoritative (UF-102, UF-302)
- [ ] invalid wires are refused, never coerced (UF-503)
- [ ] text boxes act as trigger/sink in the run panel (UF-509, UF-511)
- [ ] Run spawns/focuses a terminal session and highlights current blocks without slowing normal execution (UF-311, UF-513)

---

## Decisions waiting on Frank

- [x] **Q1** `matrix<3>[3,3,3]` — rank is `<r>`, element type via `of` → D-006
- [x] **Q2** `*` = matmul, `.*` = Hadamard (ggml semantics) → D-007
- [x] **Q7** `f.(in) g(in)` = Verilog-style port chain → D-008
- [x] **Precision** — `float<n>`, `bfloat<16>`, `quant<fmt>` → D-009
- [x] **Q14** — `.` self, `..` parent, `...` grandparent; GGUF is nested `structured` → D-011
- [x] **`until`** — do-until feedback, pass back only when unmet → D-010
- [x] **I/O terminals** — `input` trigger, `output` sink, text only → D-012
- [x] **Run observation** — Run session + coarse highlighting + optional slow mode → D-013
- [x] **Workspace/layers** — folder workspace, lazy inspection, inline/binary matrix layers → D-014
- [ ] **Q3** `sheet` — typed columns? row mutation?
- [ ] **Q4** integer representation of atomics; float comparison semantics
- [ ] **Q5** `string` ordering: equality only, or lexicographic?
- [ ] **Q6** `condition` capture rules (value vs reference)
- [ ] **Q8** portal export naming (`main` vs `export` marker)
- [ ] **Q9** do T zones imply real parallelism or only legal scheduling?
- [ ] **Q10** default fuel; `@fuel` scope
- [ ] **Q11** `.txt` truly inert, or `@doc` attachments?
- [ ] **Q12** project root entry point
- [ ] **Q13** `import` vs `portal` — semantic or UX difference?
- [ ] **Q15** is ggml mandatory or optional with a C fallback?

---

## Definition of done (every task)

- [ ] acceptance criteria above are met
- [ ] tests added; `cargo test --workspace` and `npm run build` pass
- [ ] no new warnings from `clippy -D warnings`
- [ ] spec, grammar, decision log and examples updated if semantics changed
- [ ] `TASKS.md` status updated in the same change
- [ ] recommendation or code path is traceable to a requirement above

## Ground rules

- Everything runs locally. Only synthetic data and Frank's own plans.
- The lab demonstrates failure modes; it does not deploy them.
- Anything that escapes the lab is a bug, not a feature.
