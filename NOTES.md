# Autonomous Implementation Ledger

**Rule:** Do not report the project complete until every acceptance box in
[`CHECKLIST.md`](./CHECKLIST.md) is checked with executable tests. A written
specification, grammar production, placeholder crate, or UI mock does **not**
count as implementation.

## Current verified state

- `cargo test --workspace` passes.
- `ufp tokens` and `ufp parse` provide a source-spanned lexer and shallow
  top-level parser.
- `ufp run examples/text_io.ufp` executes only direct `input.text → output.text`
  pipes, including transient `--set name=value` values.
- `ufp compile examples/text_io.ufp /tmp/program` emits C and builds a native
  executable through `cc`, only for the direct text-terminal slice.
- ggml is pinned at `third_party/ggml` and builds through
  `scripts/build-ggml.sh`; it is **not linked into UFP codegen yet**.
- The type crate has the closed type model plus text-terminal static checks;
  expression typing, inference, constraints and tensor values are not present.

## Mandatory completion order

Follow dependency order in `TASKS.md`; update `CHECKLIST.md`, specs and tests in
the same commit.

1. **M0 syntax:** complete UF-101 through UF-109. Replace the shallow parser
   with a full AST parser; parse every example and error corpus; implement
   diagnostics/recovery, formatter, matrix/binary declarations and terminals.
2. **M1 language:** complete UF-201 through UF-310. Build scopes, expression
   typing, deterministic constraints, hierarchical IR, lowering, portal module
   graph, fold semantics, interpreter and trigger protocol.
3. **M1 zones/utilities:** complete UF-401 through UF-404 and UF-701 through
   UF-709. Implement zones, builtins, sandboxed binary/JSON/CSV/GGUF loaders,
   GGUF-to-UFP exporter, condition capture and examples.
4. **M2 editor:** create the web application only after IR/checker APIs exist.
   Complete UF-501 through UF-514: folder navigation, graph/text dual view,
   drag/drop links validated by the checker, nested portal tabs, text sessions,
   bounded execution highlighting and zone bands.
5. **M3 compiler:** complete UF-601 through UF-611. Define ABI, typed C
   emitter, real ggml partition/linking, folds, portals, driver/cache, native
   text terminal entrypoint and differential tests.
6. **M4/M5:** complete demos, conformance, fuzzing, CI, documentation and
   reproducibility before declaring release readiness.

## Non-negotiable design rules

- `.txt` remains inert; `.ufp` is the only executable source.
- Text and canvas share one IR; no editor-only semantic state.
- Portals/layers/structures/folds stay compact and expand only on explicit
  inspection. Matrix inspection defaults to metadata and bounded slices.
- `*` is matmul; `.*` is element-wise. `matrix<r>` is rank; `of T` is element.
- `run … until` is do-until with gated feedback and finite fuel.
- `.`/`..`/`...` are static scope paths.
- Run animation must never slow normal execution: bounded coarse events by
  default; delay/step are opt-in presentation controls.
- Treat input/output terminal session values as transient unless explicitly
  saved back to source.

## Before each commit

1. Run narrow tests, then `cargo test --workspace`.
2. Run the relevant CLI/editor/native smoke path.
3. Update formal spec, grammar and decision log for semantic changes.
4. Check acceptance boxes only when their exact criteria and tests are met.
5. Use a focused conventional commit with a UF task ID and push `main`.

## Delegated-task repair protocol

When a delegated task finishes:

1. Inspect its diff, task acceptance boxes, and source/spec consistency.
2. Run its narrow tests, then `cargo test --workspace`; run frontend/native smoke
   commands when applicable.
3. If it passes, merge/cherry-pick its commit into `main`, resolve conflicts
   explicitly, rerun integration checks, and commit any integration repair.
4. If it fails, record the exact failing command/output and affected paths in
   the follow-up prompt. Relaunch or continue the same isolated agent with:
   - the failed acceptance criteria and task ID;
   - minimal reproduction/expected behaviour;
   - files and interfaces it may change;
   - explicit requirement to add a regression test and commit the repair.
5. Never merge a “mostly working” branch merely to advance a checklist item.

Each worker owns one worktree/branch. The integrator owns `main`; workers must
not force-push, modify submodules, or edit unrelated task areas.
