# Uncle Frank's ML Lab

> *"It's alive... and it's overfitting."*

Uncle Frank is **Frankenstein**—Victor Frankenstein's less responsible cousin.
He builds things in a lab, and the things he builds cause havoc.

This repo is two projects sharing a slab:

1. **The lab** — a deliberately cursed ML sandbox: hand-rolled tensors, a graph
   runtime, and data-level horrors (poisoning, label flipping, backdoors).
2. **Uncle Frank's Plan (`.ufp`)** — the language and graph ecosystem that will
   describe those experiments: nodes as functions, pipes as data paths, `Portal`
   for composing plans, `@T(...)` time zones, and a compiler to C + ggml.

The long-term shape: edit a `.ufp` plan visually, drill through portals, watch
the schedule render as zones, then compile the whole thing and run it.

## Status

| Piece | State |
|---|---|
| `frank-backend` (toy runtime, JSON graph API) | builds, trains, curses work |
| `.ufp` normative spec + grammar | drafted (`docs/UFP-SPEC.md`, `docs/grammar.ebnf`) |
| Cargo workspace | scaffolded (`ufp-syntax`, `ufp-types`, `ufp-ir`, `ufp-runtime`, `ufp-codegen`, `ufp-cli`) |
| Lexer / shallow plan parser | in progress (`ufp tokens`, `ufp parse`) |
| Text-terminal runtime slice | works (`ufp run`) for direct text pipes |
| Native compilation slice | works (`ufp compile`) for direct text pipes; no ggml yet |
| Editor / type checker / full IR | not started |

The honest task list is in [`CHECKLIST.md`](./CHECKLIST.md); the index with sizes
and dependencies is [`TASKS.md`](./TASKS.md).

## Layout

```
backend/          Rust reference executor (axum + hand-rolled tensors)
docs/
  UFP-SPEC.md     normative `.ufp` language specification
  SPEC.md         design/system overview (draft v0.1)
  grammar.ebnf    formal grammar (draft v0.1)
  RUNTIME.md      current backend API and semantics
  DECISIONS.md    decision log (Q1–Q15)
examples/         aspirational .ufp plans + one inert .txt
crates/          `.ufp` compiler workspace crates
CHECKLIST.md      original-brief traceability + acceptance criteria
TASKS.md          task ledger, epics E0–E9, milestones M0–M5
Contribute.md     contributor workflow, commit and PR guidelines
third_party/ggml/ pinned upstream ggml submodule (initialise recursively)
scripts/         local build helpers
```

## Quickstart (today)

```sh
cd backend
cargo run
# GET  http://127.0.0.1:8787/api/health
# GET  http://127.0.0.1:8787/api/graph/demo
# POST http://127.0.0.1:8787/api/graph/run   {"graph": {...}, "steps": 300}
```

Details, graph JSON, and the measured effect of each curse: [`docs/RUNTIME.md`](./docs/RUNTIME.md).

## UFP vertical slice

```sh
# Parse a plan, run direct input.text → output.text pipes, or emit native C.
cargo run -p ufp-cli -- parse examples/text_io.ufp
cargo run -p ufp-cli -- run examples/text_io.ufp --set 'prompt=hello from a session'
cargo run -p ufp-cli -- compile examples/text_io.ufp /tmp/text_io
/tmp/text_io
```

This is intentionally a small, honest slice: procedures, matrices, portals,
GGUF, loaders, editor and ggml compilation are not executable yet. See the
unchecked criteria in `CHECKLIST.md`.

For the pinned ggml dependency and native build instructions, see
[`docs/BUILD.md`](./docs/BUILD.md).

## The two file kinds

- **`.ufp`** — Uncle Frank's Plan. A typed dataflow graph with procedures, pipes,
  portals, and time-zone annotations. Text and canvas are two views of one IR.
- **`.txt`** — inert. Notes, marginalia, apologies. Never compiled.

## Ground rules

- Everything runs locally on synthetic data (and, later, models you supply).
- No third-party system is targeted, probed, or "experimented on".
- The lab is for understanding failure modes, not deploying them.
- Anything that escapes the lab is a bug, not a feature.

## License

Not chosen yet (UF-003). Until then, all rights reserved by the author.
