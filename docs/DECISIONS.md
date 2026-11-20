# Decision Log

Semantics that the spec leaves open, plus the calls already made. One entry per
question; `open` means the compiler and editor must not hard-code an answer yet.

## Format

```
### D-### — title
Status: decided · open
Decision: ...
Rationale: ...
Spec: §x / Qn
```

---

## Decided

### D-001 — `.txt` files are inert
**Status:** decided
**Decision:** `.txt` files are carried in a project but never parsed as plans,
imported, or type-checked. The editor may display them as notes.
**Rationale:** Frank asked for a file kind that does nothing; keeping it truly
inert prevents accidental semantics.
**Spec:** §1, Q11 (adjacent)

### D-002 — no general loops
**Status:** decided
**Decision:** the only iteration is `run … until`, a bounded fold with a fuel
bound. `for`/`while` do not exist.
**Rationale:** keeps lowering to ggml/C tractable and termination checkable.
**Spec:** §4.3

### D-003 — the graph is a DAG; the fold is the only cycle
**Status:** decided
**Decision:** plain dataflow edges may not form cycles. A fold node contains its
body as a nested region with a labelled self-edge.
**Rationale:** one legal cycle shape is easy to validate and render.
**Spec:** §2.2, §7

### D-004 — the IR is the single source of truth
**Status:** decided
**Decision:** text and canvas are views over the same IR; the editor never owns a
second model. Comments and canvas positions live in side tables keyed by stable ids.
**Rationale:** avoids drift between editor state and compiler input.
**Spec:** §7, §11

### D-005 — the reference executor stays
**Status:** decided
**Decision:** `frank-backend` remains buildable as the reference executor while
the `.ufp` toolchain is built; interpreter and native output are tested against it.
**Rationale:** gives the project something that runs from day one.
**Spec:** none (engineering)

---

## Open

### Q1 — `matrix<3>[3,3]`: rank or element type?
**Status:** open
**Options:** (a) `<3>` = rank assertion, canonical `matrix<float>[3,3]`;
(b) `<3>` = element type (invalid as written); (c) drop `<…>` entirely.
**Leaning:** (a), with a diagnostic when the assertion disagrees with the dims.
**Spec:** §3.1

### Q2 — `*` on matrices: element-wise or matmul?
**Status:** open
**Options:** NumPy (`*` element-wise, `@` matmul) vs maths (`*` matmul).
**Leaning:** NumPy rules; `@` is unambiguous.
**Spec:** §4.1

### Q3 — `sheet` semantics
**Status:** open
**Options:** typed columns only (read-only rows) vs mutable rows and columns.
**Leaning:** typed columns, row read returns `structured`; mutation via explicit
`sheet_set` builtins.
**Spec:** §3.1

### Q4 — integer representation of atomics
**Status:** open
**Question:** are floats bit-cast to integers for comparison, or compared
numerically? What does ordering a `structured{float a}` mean?
**Leaning:** numeric comparison for floats; structured ordering is field-wise and
requires every field to be orderable.
**Spec:** §3.3

### Q5 — `string` ordering
**Status:** open
**Options:** equality only vs lexicographic ordering.
**Leaning:** equality only; ordering strings in a dataflow language invites
locale bugs.
**Spec:** §3.3

### Q6 — `condition` capture rules
**Status:** open
**Options:** capture by value, by reference, or explicit capture list.
**Leaning:** explicit-by-default, inferred from free variables with a printed
capture set in diagnostics.
**Spec:** §3.4

### Q7 — `f.(a) g(b)` desugaring
**Status:** open
**Options:** result becomes `g`'s first free argument; result is threaded as a
named `it`; operator form `f |> g`.
**Leaning:** first-free-argument insertion, rendered as an explicit edge.
**Spec:** §4.4

### Q8 — portal export naming
**Status:** open
**Options:** target's `main` by default vs required `export procedure` marker.
**Leaning:** `main` default, optional `as` to select another exported procedure.
**Spec:** §5

### Q9 — do T zones imply parallelism?
**Status:** open
**Options:** legal scheduling only vs real threads/barriers.
**Leaning:** scheduling annotation; `--schedule=strict` inserts barriers, but no
implicit threading.
**Spec:** §8

### Q10 — fold fuel
**Status:** open
**Options:** fixed default (10 000) vs required annotation vs plan-wide `@fuel`.
**Leaning:** default 10 000 with per-fold `@fuel(...)` override.
**Spec:** §4.3

### Q11 — `.txt` as `@doc` attachments
**Status:** open
**Options:** truly inert vs attachable to nodes as documentation.
**Leaning:** inert file, with an optional `@doc("notes.txt")` annotation later.
**Spec:** §1, D-001

### Q12 — project entry point
**Status:** open
**Options:** `main` in the open file vs an explicit `project.ufp` manifest.
**Leaning:** explicit manifest once multi-file projects land.
**Spec:** §5

### Q13 — `import` vs `portal`
**Status:** open
**Options:** semantic difference vs UX only.
**Leaning:** `import` = compiler builtins; `portal` = another user plan. Different
namespaces, same pin model.
**Spec:** §5, §10

### Q14 — scope of `..`
**Status:** open
**Options:** enclosing procedure's result vs caller's result.
**Leaning:** enclosing procedure's result, read-only.
**Spec:** §4.3

### Q15 — ggml required?
**Status:** open
**Options:** hard dependency vs optional backend with a pure-C fallback.
**Leaning:** required for the tensor partition; scalar-only plans compile without it.
**Spec:** §9
