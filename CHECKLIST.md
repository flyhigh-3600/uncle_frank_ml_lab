# Uncle Frank's Lab — Working Checklist

The executable view of the project. Task IDs match [`TASKS.md`](./TASKS.md) and
the GitHub epics. Spec: [`docs/SPEC.md`](./docs/SPEC.md) ·
Grammar: [`docs/grammar.ebnf`](./docs/grammar.ebnf).

**Current focus:** M0 — skeleton and parser. `frank-backend` builds and trains;
nothing else is wired yet.

---

## Start here (M0)

- [ ] **UF-001** Cargo workspace restructure
  - [ ] crates `ufp-syntax`, `ufp-types`, `ufp-ir`, `ufp-runtime`, `ufp-codegen`, `ufp-cli`
  - [ ] `frank-backend` stays buildable and keeps its JSON API
  - [ ] `cargo test --workspace` passes from a clean checkout
- [ ] **UF-002** CI: fmt + clippy + test on push; web build job
- [ ] **UF-003** `.gitignore` (targets, node_modules, dist), license decision, CONTRIBUTING, issue/PR templates
- [ ] **UF-005** Decision log opened with Q1–Q15 from the spec
- [~] **UF-004** Reference executor documented (`docs/RUNTIME.md`: endpoints, graph JSON, curses)
- [ ] **UF-101** Lexer
  - [ ] every token in `grammar.ebnf` §8 lexes with accurate spans
  - [ ] comments, strings, `int<32>`, `matrix<3>[3,3,3]`, `@T(0.1.2)`, `.`/`..`
  - [ ] error tokens never panic; fuzz-safe
- [ ] **UF-102** Plan-level parser
  - [ ] `import`, `portal`, `text`, `procedure`, `node`, `pipe`, annotations
  - [ ] round-trips the example corpus (once UF-105 lands)
- [ ] **UF-103** Statement/expression parser
  - [ ] `if/elif/else`, `run … until`, `structured{…}`, matrix/list literals, index, field, `f.(a) g(b)`
  - [ ] AST preserves spans for every node
- [ ] **UF-104** Diagnostics & recovery
  - [ ] `UFP1xxx` codes, span + hint, deterministic ordering
  - [ ] parser continues after a bad declaration and reports all errors

---

## E0 — Foundations

- [ ] **UF-001** workspace restructure — *see above*
- [ ] **UF-002** CI baseline — *see above*
- [ ] **UF-003** repo hygiene — *see above*
- [ ] **UF-004** runtime preserved + documented — *see above*
- [ ] **UF-005** `docs/DECISIONS.md` with one entry per open question, owner, date

## E1 — Syntax

- [ ] **UF-101** lexer — *see above*
- [ ] **UF-102** plan parser — *see above*
- [ ] **UF-103** expression parser — *see above*
- [ ] **UF-104** diagnostics — *see above*
- [ ] **UF-105** golden tests
  - [ ] one `.ufp` per language feature under `tests/corpus/`
  - [ ] AST snapshot diffs reviewed, not blind-accepted
- [ ] **UF-106** formatter
  - [ ] `format(parse(src))` is idempotent
  - [ ] comments and annotations survive a round-trip
- [ ] **UF-107** `.txt` attachments
  - [ ] parsed into the project manifest, never type-checked
  - [ ] editor shows them as notes only

## E2 — Types & constraints

- [ ] **UF-201** type model — all §3.1 types, interning, canonical printing; `matrix<r>` = rank, `of` = element (D-006); `float<n>`, `bfloat<16>`, `quant<fmt>` (D-009)
- [ ] **UF-202** scopes & symbols — `.`/`..`/`...` outer-scope walking (D-011), readonly; shadowing rules documented and tested
- [ ] **UF-203** expression typing
  - [ ] index arity = rank; bounds provable → static, else runtime check
  - [ ] dot access works for `structured` and `sheet`
  - [ ] operators follow §4.1; shape mismatch reports both shapes
- [ ] **UF-204** comparisons
  - [ ] matrix → bool matrix, structured → bool structured
  - [ ] ordering a `string` field is an error; `==` is allowed
- [ ] **UF-205** pins — explicit, `auto` (fixpoint), `inout`; wire-time re-check
- [ ] **UF-206** constraint solver
  - [ ] `with a.shape = b.shape` solves across pins and pipes
  - [ ] contradiction gives one deterministic, minimal error
  - [ ] property test: solver never accepts an ill-typed system
- [ ] **UF-207** diagnostic catalogue

## E3 — IR & lowering

- [ ] **UF-301** IR schema — hierarchical, typed ports, zones, source spans, stable ids
- [ ] **UF-302** straight-line + pipes lowering
- [ ] **UF-303** conditionals → predicate/branch/merge; branch type unification
- [ ] **UF-304** `run … until` → fold node (do-until, D-010)
  - [ ] body runs ≥ 1; feedback edge gated on "condition unmet"
  - [ ] fuel default 10 000, `@fuel` override
  - [ ] provably unbounded → compile error
- [ ] **UF-305** portals
  - [ ] pin signature derived from target export
  - [ ] cycle detected and reported with the cycle path
  - [ ] cache key = content hash + compiler version + flags
- [ ] **UF-306** structured/list/sheet lowering
- [ ] **UF-307** IR validation — fan-in, cycles, zone order, type agreement
- [ ] **UF-308** editor JSON round-trip — structural equality after save/load
- [ ] **UF-309** interpreter
  - [ ] runs every example plan
  - [ ] float tolerances documented per builtin

## E4 — Time zones

- [ ] **UF-401** parse `@T(l.m.n)`; reject negatives/empty segments
- [ ] **UF-402** semantics: nesting, phase order, sibling concurrency; tests in `docs/SPEC.md` §8
- [ ] **UF-403** ASAP inference for un-annotated nodes
- [ ] **UF-404** validation: zone inversion errors, duplicate/illegal paths, `@concurrent` races
- [ ] **UF-405** editor bands, legend, per-zone status, collapse

## E5 — Editor

- [ ] **UF-501** web scaffold + lab theme + three-pane layout
- [ ] **UF-502** node/port rendering with type + shape badges; deterministic port geometry
- [ ] **UF-503** drag-drop wiring
  - [ ] link accepted only if checker accepts
  - [ ] refusal shows type/constraint/zone reason inline
  - [ ] no silent coercion, ever
- [ ] **UF-504** inspector — pins, types, constraints, params, source span, zone
- [ ] **UF-505** portal drill-in
  - [ ] double-click opens nested plan in a tab, breadcrumb back
  - [ ] target signature change triggers migration diagnostics on the caller
- [ ] **UF-506** pipe editing — reroute, rename, delete, fan-out preserved
- [ ] **UF-507** undo/redo, multi-select, copy/paste, autosave
- [ ] **UF-508** project workspace — file tree, multiple `.ufp`, `.txt` notes pane
- [ ] **UF-509** compile/run panel — streamed diagnostics, build log, artifact links
- [ ] **UF-510** shortcuts + a11y pass

## E6 — Compiler (C + ggml)

- [ ] **UF-601** C runtime header — arena, views, ownership, length-prefixed strings/binary
- [ ] **UF-602** C codegen — scalars, control flow, structured, string/binary
- [ ] **UF-603** matrix partitioner + ggml mapping (`*` → `mul_mat`, `.*` → `mul`, add/sub/div, transpose, softmax; quantized matmul)
- [ ] **UF-604** ggml build integration; artifacts deterministic and hashed
- [ ] **UF-605** fold codegen — bounded loop, inlined/again body, fuel error
- [ ] **UF-606** portal translation units + linking
- [ ] **UF-607** driver — clang invocation, flags, cache manifest, `--schedule=strict|relaxed`
- [ ] **UF-608** differential tests interpreter vs native, documented tolerances
- [ ] **UF-609** sandbox — closed builtin registry, no raw C/asm/fs escape

## E7 — ML utilities & examples

- [ ] **UF-701** builtin registry with typed pins + capabilities
- [ ] **UF-702** `import_gguf` — nested `structured` (metadata + matrix substructures), per-file type specialisation, dtype mapping
- [ ] **UF-703** `condition` combinators + capture checks
- [ ] **UF-704** example corpus (one plan per feature, all parsed by CI)
- [ ] **UF-705** end-to-end: GGUF → fold generation → decoded text

## E8 — QA

- [ ] **UF-801** unit + golden suites across crates
- [ ] **UF-802** solver property tests
- [ ] **UF-803** lowering snapshots (one per rule)
- [ ] **UF-804** parser fuzzing
- [ ] **UF-805** CI matrix + release checklist

## E9 — Docs & community

- [ ] **UF-901** language reference (mdBook)
- [ ] **UF-902** tutorial: first plan → compile → run
- [ ] **UF-903** architecture & compiler internals
- [ ] **UF-904** contribution guide + roadmap sync

---

## Decisions waiting on Frank

- [x] **Q1** `matrix<3>[3,3,3]` — rank is `<r>`, element type via `of` → D-006
- [x] **Q2** `*` = matmul, `.*` = Hadamard (ggml semantics) → D-007
- [x] **Q7** `f.(in) g(in)` = Verilog-style port chain → D-008
- [x] **Precision** — `float<n>`, `bfloat<16>`, `quant<fmt>` → D-009
- [x] **Q14** — `.` self, `..` parent, `...` grandparent; GGUF is nested `structured` → D-011
- [x] **`until`** — do-until feedback, pass back only when unmet → D-010
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

## Definition of done (every task)

- [ ] acceptance criteria above are met
- [ ] tests added; `cargo test --workspace` and `npm run build` pass
- [ ] no new warnings from `clippy -D warnings`
- [ ] spec/grammar updated if semantics changed
- [ ] `TASKS.md` status updated in the same PR

## Ground rules

- Everything runs locally. Only synthetic data and Frank's own plans.
- The lab demonstrates failure modes; it does not deploy them.
- Anything that escapes the lab is a bug, not a feature.
