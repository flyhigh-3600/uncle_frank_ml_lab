# Contributing to Uncle Frank's ML Lab

Thank you for helping build Uncle Frank's ML Lab. This repository contains both
an intentionally cursed local ML sandbox and **Uncle Frank's Plan** (`.ufp`), a
small HDL-like graph language. Contributions are welcome when they keep the
project local, inspectable, deterministic, and consistent between source, graph,
editor, interpreter, and compiler.

> **Safety boundary:** this is a local educational lab for synthetic data and
> models the user supplies. Do not add remote probing, credential handling,
> stealth, persistence, deployment, or functionality intended to affect systems
> outside the lab.

## Start here

Read these in order before changing semantics:

1. [`README.md`](./README.md) — project layout and current state.
2. [`CHECKLIST.md`](./CHECKLIST.md) — complete acceptance checklist, mapped to
   the original brief.
3. [`TASKS.md`](./TASKS.md) — dependency-ordered work ledger and task IDs.
4. [`docs/UFP-SPEC.md`](./docs/UFP-SPEC.md) — normative `.ufp` specification.
5. [`docs/grammar.ebnf`](./docs/grammar.ebnf) — normative grammar.
6. [`docs/DECISIONS.md`](./docs/DECISIONS.md) — decisions that are already
   settled; do not silently reopen them in code.
7. [`docs/RUNTIME.md`](./docs/RUNTIME.md) — current `frank-backend` reference
   executor API.

Use a task ID in your branch, commit, PR, and checklist updates, for example
`UF-101 lexer spans`.

## What belongs where

| Path | Responsibility |
|---|---|
| `backend/` | Current Axum/reference ML graph executor. Keep it buildable. |
| `crates/ufp-syntax/` | Lexer, parser, AST, spans, syntax diagnostics. |
| `crates/ufp-types/` | Types, names/scopes, constraints, static diagnostics. |
| `crates/ufp-ir/` | Shared hierarchical dataflow IR and validation. |
| `crates/ufp-runtime/` | `.ufp` reference interpreter and trigger protocol. |
| `crates/ufp-codegen/` | C + ggml partitioning and code generation. |
| `crates/ufp-cli/` | `ufp` command-line driver. |
| `docs/` | Normative specification, decisions, runtime contract. |
| `examples/` | Small, focused `.ufp` examples and inert `.txt` notes. |
| future `frontend/` | Browser graph editor; it must use the shared IR/checker. |

## Development setup

Requirements today:

- current stable Rust toolchain (`rustup`, `cargo`, `rustfmt`, `clippy`);
- Node LTS/npm once the frontend lands;
- `clang` and ggml only for native compiler work (E6).

```sh
git clone https://github.com/flyhigh-3600/uncle_frank_ml_lab.git
cd uncle_frank_ml_lab
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Run the current reference service separately if needed:

```sh
cd backend
FRANK_ADDR=127.0.0.1:8899 cargo run
```

Port `8787` may already be occupied in some development environments.

## Pick and claim work

1. Choose a task in `TASKS.md` whose dependencies are done.
2. Check the matching acceptance boxes in `CHECKLIST.md`.
3. Use the existing GitHub epic issue, or open/claim a focused issue that cites
   the task ID.
4. Create a branch such as `uf-101-lexer-spans` or `docs-ufp-spec-io`.
5. Keep one branch/PR focused on one task or closely coupled change.

Do not mark a task `done` merely because the source compiles. A task is done only
when its acceptance criteria, tests, documentation, and status are all updated.

## Required design rules

These are architectural invariants, not style preferences:

- **One IR:** source text and graph canvas are views over the same IR. The editor
  must not create a second semantic model.
- **Typed wiring:** an input port has at most one incoming pipe; fan-out is okay.
  Invalid editor links must be refused with the checker’s reason, never coerced.
- **Graph shape:** ordinary dataflow is a DAG. `run … until` is the only cycle:
  a do-until fold with a gated feedback edge and finite fuel.
- **Portals:** compose user `.ufp` plans and are inspectable/editable; `import`
  is for compiler-known utilities.
- **Files:** `.ufp` has semantics; `.txt` is inert and is never compiled.
- **No unapproved types:** do not invent model, GGUF, image, or other type
  constructors. A GGUF is nested `structured` + `list` + `matrix`.
- **Scope paths:** `.` is self; `..` parent; `...` grandparent. Resolution is
  static and writes through an outer scope are rejected.
- **Matrix rules:** `matrix<r>` names rank; `of T` names elements. `*` means
  matrix multiplication and `.*` means element-wise multiplication.
- **Text terminals:** `input name = "…";` is a text source and runtime trigger;
  `output name;` is a text sink. Keep inputs text-only until D-012 changes.
- **Run observation:** normal execution emits only bounded, coarse lifecycle
  events. Do not add per-tensor copying, synchronous canvas rendering, or
  animation delays to the hot path. Slow/step modes are opt-in presentation
  controls and must not change values, scheduling, fuel, or determinism (D-013).
- **Determinism:** same source, inputs, seed, compiler version and flags should
  give the same IR, diagnostics and results within documented numeric tolerance.

If a change must alter one of these, propose it in an issue first and add a dated
entry to `docs/DECISIONS.md` after it is ruled.

## Implementation workflow

### 1. Make the smallest coherent change

Avoid drive-by formatting and unrelated refactors. Do not reset, revert, or
remove work you did not author. Do not change the reference runtime’s public API
without documenting a compatibility plan.

### 2. Update the language contract with semantic changes

When behaviour changes, update all applicable files in the same PR:

| Change | Update |
|---|---|
| Accepted/rejected syntax | `docs/grammar.ebnf`, `docs/UFP-SPEC.md`, parser tests |
| Meaning/type/lowering | `docs/UFP-SPEC.md`, `docs/SPEC.md`, `DECISIONS.md`, tests |
| New decision/ruling | `docs/DECISIONS.md`, checklist, examples |
| Task status/acceptance | `TASKS.md`, `CHECKLIST.md` |
| Runtime HTTP API | `docs/RUNTIME.md`, endpoint tests |
| New user-visible feature | focused example under `examples/`, README table |

The normative spec wins over the design overview; the grammar wins for syntax.
If they conflict, fix the conflict rather than choosing quietly in code.

### 3. Test locally

Run the narrow test first, then the workspace checks:

```sh
cargo test -p ufp-syntax
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

For backend changes also run:

```sh
cd backend
cargo build
```

When the frontend exists, run its formatter/linter/tests/build and record the
commands in the PR. For parser work, add a success corpus case and an error case
that asserts code and span. For semantic work, add a lowering/interpreter test.
For editor wiring, add an end-to-end test for acceptance and refusal.

### 4. Keep diagnostics stable

Diagnostics use `UFP####` codes. New codes need a documented range and an entry
in the catalogue. Tests should assert the code and relevant span, not fragile
full prose. Diagnostics must be deterministic in source order.

## Commit guidelines

Use focused, imperative conventional commit messages with a task ID when there
is one:

```text
<type>(<scope>): <imperative summary> [UF-###]
```

Allowed types:

| Type | Use |
|---|---|
| `feat` | User-visible language/editor/runtime capability |
| `fix` | Correct incorrect behaviour |
| `docs` | Specification, checklist, examples, contributor docs |
| `test` | Tests only |
| `refactor` | Internal restructuring with unchanged behaviour |
| `build` | Cargo, CI, dependency, build-system work |
| `chore` | Maintenance not covered above |

Examples:

```text
feat(syntax): lex raw text terminals [UF-101]
feat(runtime): capture text terminal outputs [UF-310]
fix(types): reject writes through parent scope [UF-202]
docs(spec): define text input trigger semantics [UF-108]
test(parser): cover portal cycle diagnostics [UF-105]
build(workspace): add UFP crate skeleton [UF-001]
```

Rules:

- Subject is ≤72 characters where practical, imperative, no trailing period.
- One logical change per commit. Keep formatting-only changes separate.
- A body is required for non-obvious changes: explain **why**, compatibility,
  tests run, and any remaining limitation.
- Never put secrets, local paths, generated `target/`, `node_modules/`, model
  weights, or binary build artifacts in a commit.
- Do not amend/rewrite a branch another contributor may be using without their
  explicit agreement.

Recommended body/trailer form:

```text
feat(syntax): add text terminal declarations [UF-108]

Parse top-level input/output declarations and retain the source span.
They remain string-only and lower later in UF-302.

Tests: cargo test -p ufp-syntax
Refs: #<issue-number>
```

## Pull request guidelines

A PR description must include:

- linked issue and task ID(s);
- short problem and solution summary;
- semantic/spec changes, or “none”;
- tests and commands run;
- screenshots/video for editor changes;
- known limitations or follow-up tasks;
- checklist updates and the reason any boxes remain unchecked.

PRs should be reviewable. Prefer a small series (`lexer` → `parser` → `AST`
formatting) over one giant “language implementation” pull request. Do not merge
with failing tests, unformatted Rust, unresolved semantic contradictions, or
silent changes to examples.

## Reporting bugs and proposing language changes

For a bug, include:

- minimal `.ufp` source or graph JSON;
- expected and actual result;
- exact command/version/platform;
- diagnostic code/span or runtime trace;
- whether the interpreter, native output, editor, or all disagree.

For a language change, include:

- problem and a minimal before/after example;
- parser/typing/lowering/runtime/editor impact;
- compatibility and migration impact;
- update proposed for `UFP-SPEC.md`, `grammar.ebnf`, and `DECISIONS.md`.

Do **not** implement unresolved questions (Q3–Q6, Q8–Q13, Q15) as if they were
settled. Open an issue and request a ruling first.

## Definition of a good contribution

A good contribution is small enough to understand, has tests that demonstrate
its contract, updates the one true spec/grammar/decision record, preserves the
shared IR model, and leaves the lab safer and more reproducible than it found it.

Thank you for keeping Frank’s laboratory alive — but contained.
