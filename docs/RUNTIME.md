# Reference Executor (the current `frank-backend`)

Before the `.ufp` toolchain exists, the lab still needs something that can chew on
a graph. `backend/` is a small, dependency-light Rust service with hand-rolled
tensors, backprop, and a few curses. It is the **reference executor**: the
interpreter (UF-309) and the native compiler (E6) must eventually match it.

It is intentionally not the final architecture — UF-001 moves it into a workspace.

## Run it

```sh
cd backend
cargo run                 # listens on 127.0.0.1:8787 by default
FRANK_ADDR=127.0.0.1:8899 cargo run        # if 8787 is taken
FRANK_STATIC_DIR=/path/to/dist cargo run   # serve a built editor
```

## API

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/health` | — | lab name, version, motto |
| GET | `/api/ops` | — | op catalogue (pins, params, categories) |
| GET | `/api/graph/demo` | — | starter graph (`dataset → poison → dense → CE`) |
| POST | `/api/graph/validate` | graph | `ok`, `errors`, `warnings`, resolved `shapes` |
| POST | `/api/graph/run` | `{graph, steps, lr, seed}` | loss/accuracy history, probes, curse notes |

Errors from validation are readable sentences, not codes — the compiler will fix
that with `UFP####` diagnostics (UF-207).

## Graph JSON

```json
{
  "nodes": [{ "id": "n1", "kind": "dataset", "x": 60, "y": 120,
              "params": { "dataset": "moons", "n": 200, "noise": 0.15 } }],
  "edges": [{ "id": "e1", "from": "n1", "from_port": "x",
              "to": "n2", "to_port": "x" }]
}
```

Node positions are editor hints; the backend ignores them. Ports are resolved by
name; a port may have at most one incoming edge.

## Ops today

`dataset` (moons/blobs/xor) · `dense` (he/xavier/tiny/chaos init) · `relu` ·
`tanh` · `softmax` · `ce` · `mse` · `poison` · `label_flip` · `backdoor` ·
`inspect`

Hyperparameters live as clamped JSON params (`units`, `rate`, `scale`, `init`, …)
so the editor and runtime cannot disagree.

## Run semantics (today)

- Full-batch SGD, `steps` iterations, `lr` clamped to `[0, 10]`, weights seeded by
  `(node id, seed)` — same graph + same seed ⇒ identical history.
- Exactly one loss node drives training. Extra losses are a warning, not a sum.
- Data is generated once per run; curses mutate it deterministically.
- Backprop is hand-written and accumulates gradients across branching graphs;
  data-source nodes stop propagation.
- Divergence (NaN/∞) stops the run and is reported, not hidden.

## Cursed effects, measured

The demo graph (15% poison, σ = 2.0, 300 steps, lr 0.3) goes from
**loss 1.45 / accuracy 27%** to **loss 0.56 / accuracy 73%** — the model learns,
but the rot costs it the clean-data ceiling. That gap is the point.

## Tests

There are no unit tests yet (UF-801). The smoke path currently used:

```sh
curl -s localhost:8787/api/graph/demo > /tmp/demo.json
curl -s -X POST localhost:8787/api/graph/validate \
     -H 'content-type: application/json' -d @/tmp/demo.json
```
