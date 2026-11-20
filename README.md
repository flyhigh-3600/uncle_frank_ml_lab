# Uncle Frank's ML Lab

> *"It's alive... and it's overfitting."*

Named after Victor Frankenstein's less responsible cousin. Uncle Frank builds
things in a lab, and the things he builds cause havoc.

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
| `.ufp` spec + grammar | drafted (`docs/SPEC.md`, `docs/grammar.ebnf`) |
| Lexer / parser / type checker | not started |
| Editor | not started |
| C + ggml compiler | not started |

The honest task list is in [`CHECKLIST.md`](./CHECKLIST.md); the index with sizes
and dependencies is [`TASKS.md`](./TASKS.md).

## Layout

```
backend/          Rust reference executor (axum + hand-rolled tensors)
docs/
  SPEC.md         language & system specification (draft v0.1)
  grammar.ebnf    formal grammar (draft v0.1)
  RUNTIME.md      current backend API and semantics
  DECISIONS.md    decision log (Q1–Q15)
examples/         aspirational .ufp plans + one inert .txt
CHECKLIST.md      working checklist with acceptance criteria
TASKS.md          task ledger, epics E0–E9, milestones M0–M4
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
