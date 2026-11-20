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

### D-006 — `matrix<r>` is the rank; `of` is the element type
**Status:** decided (Q1)
**Decision:** `matrix<3>[3,3,3]` is a rank-3 tensor with extents 3×3×3 and float
elements. Element type is written `of <type>`:
`matrix<2>[200,2] of double`. Rank is optional; when present it must equal the
number of extents. The old `matrix<float>[...]` spelling is removed.
**Rationale:** Frank's ruling: the angle brackets count dimensions.
**Spec:** §3.1

### D-007 — `*` is matmul; `.*` is element-wise
**Status:** decided (Q2)
**Decision:** `a * b` lowers to `ggml_mul_mat`; `.*` to `ggml_mul`; `/` to
`ggml_div`; `+`/`-` to `ggml_add`/`ggml_sub`. `@` is removed — one spelling per
operation. Source semantics stay row-major `a·b`; the compiler inserts the
transposes ggml's `Aᵀ·B` convention needs.
**Rationale:** Frank's ruling plus ggml's operator set.
**Spec:** §4.1

### D-008 — port chains are Verilog-style
**Status:** decided (Q7)
**Decision:** `f.(a, b) g(c)` invokes `f` with its input port map, wires every
output of `f` left-to-right into `g`'s leftmost unbound inputs, then lets `g`'s
outputs exit to the right (expression value / next link). Arguments after `g`
fill the next free input pins. It does *not* reorder arguments. The named form
`g(in0 = f(a, b), in1 = c)` is authoritative and always legal.
**Rationale:** Frank's model — inputs are given from the left, outputs leave from
the right; the whole thing reads like Verilog module instantiation.
**Caveat:** if the trailing `(tokens)` was intended as *output naming* rather
than an input, only the sugar changes; the named form and IR are unaffected.
**Spec:** §4.4

### D-009 — sized floats and ggml quantization
**Status:** decided (Frank directive)
**Decision:** element types include `float<8|16|32|64>` (8-bit needs an
`e4m3`/`e5m2` variant), `bfloat<16>`, and `quant<fmt>` for ggml block formats.
`quant<fmt>` is matrix-only, carries blocks + scales, and requires extents that
match the format's block size. Matmul with float activations is allowed
(`ggml_mul_mat`, f32 accumulation); element-wise ops and indexing require an
explicit `dequantize`, and `quantize(m, fmt)` is the inverse. Matrix literals
cannot be quantized.
**Rationale:** LLMs arrive in whatever precision their authors chose; the type
system should say so instead of pretending everything is f32.
**Spec:** §3.1

### D-010 — `run … until` is a conditional feedback loop
**Status:** decided (Frank clarification of Q7's `until`)
**Decision:** the body procedure is called first; its result is passed back as the
next input only if the `until` condition is **unmet**; if met, the loop stops and
yields the result. Body runs at least once (do-until), fuel still bounds it.
**Rationale:** "until calls the procedure again until the condition is fulfilled;
tokens are passed back only if the conditions are unmet."
**Spec:** §4.3

### D-011 — dot-scope walking; GGUF is nested `structured`
**Status:** decided (Frank clarification; resolves Q14)
**Decision:**
- `.` = self, `..` = parent scope, `...` = grandparent, one dot per level;
  a `structured` literal and a procedure body each open a scope.
- Outer reads are allowed; assigning through `..` is an error. Resolution is
  static — no runtime lookup, no dynamic scoping.
- `import_gguf` returns an ordinary nested `structured`: metadata substructure +
  heterogeneous `list` of tensor substructures (`name`, `shape`, `dtype`, `data`).
  No model type, no new type constructors. The loaded type is specialised from
  the file's metadata, so `matrices[0].data` has a concrete matrix type at a
  constant index.
**Rationale:** a GGUF is just a structure with nested substructures; matrices are
referenced across scopes with dots (e.g. `..matrices[0]`).
**Spec:** §3.2, §10.1

### D-012 — input/output are text terminals and run triggers
**Status:** decided (Frank directive)
**Decision:** a plan may declare `input name = "<text>";` and `output name;`.
They are not new types: they lower to ordinary procedures. An input is a
zero-input procedure with one `text: string` output and is the runtime **source
and trigger**; an output is a one-input `text: string` procedure with no outputs
and is the **sink**. The editor opens either as a text box — editable for inputs,
read-only for outputs — and edits rewrite the source literal. Input is string-only
for now; multiple terminals are allowed, all inputs fire together, and every
output must be wired (`UFP3404`). Text boxes may use triple-quoted raw blocks.
**Rationale:** "input and output procedures which can be opened as text boxes and
can be used as the source/trigger for the runtime; keep the input as text only
for now."
**Spec:** §2.3, §11; UFP-SPEC §6.4, §15.5

### D-013 — run sessions use coarse, opt-in observation
**Status:** decided (Frank directive)
**Decision:** clicking Run creates/focuses an interactive terminal session. It
shows editable text inputs and read-only outputs, then runs from a snapshot of
those input values. The canvas may highlight the current executing node/block
from bounded lifecycle events. Normal mode is coarse and low-overhead: no
per-tensor copies, no rendering on the runtime path, and events may be coalesced.
A user-controlled slow mode adds delay only *between* events; it does not alter
program scheduling, values, determinism, fuel, or native output. Fine-grained
stepping/tracing is opt-in and shares the same event boundaries.
**Rationale:** interactive visibility is useful, but it must not trade normal
runtime performance for animation accuracy.
**Spec:** UFP-SPEC §15.6, §17; SPEC §11

### D-014 — folder workspace, lazy inspection, and layer storage
**Status:** decided (Frank directive)
**Decision:** the opened folder is a notebook-like UFP workspace. Each `.ufp`
is an inspectable graph/source document; models SHOULD be split into layer plans
composed through compact `Portal` nodes. Portals, procedures, structures and fold
regions show a summary and expand only on explicit user inspection. Small layer
weights may use named nested matrix declarations (`matrix[3,4] layer_0 =
[[...]]`); large weights use validated, lazy project-relative binary-backed
matrices. Opaque `binary` values support byte-array and local byte-file loading.
**Rationale:** deep inspection must be accurate without making a large model
canvas or runtime unusably heavy.
**Spec:** UFP-SPEC §4.3, §17; SPEC §10.2

---

## Open

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

### Q15 — ggml required?
**Status:** open
**Options:** hard dependency vs optional backend with a pure-C fallback.
**Leaning:** required for the tensor partition; scalar-only plans compile without it.
**Spec:** §9
