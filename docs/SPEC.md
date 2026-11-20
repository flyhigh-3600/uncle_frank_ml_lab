# Uncle Frank's Plan — Language & System Specification

**Status:** draft v0.1 · **Audience:** Frank (and anyone brave enough to open the lab door)

`.ufp` is Uncle Frank's Plan: an HDL-flavoured language in which every node is a
function, every function is a node, and data crawls between them through pipes.
This document is the contract the compiler, the editor, and the task ledger are
written against. The grammar lives in [`grammar.ebnf`](./grammar.ebnf).

---

## 1. Vision

A project is a set of `.ufp` plans. A plan is a typed dataflow graph that can be:

- **edited visually** — drag nodes, wire pins, drill into `Portal`s;
- **read and written as text** — the graph and the source are two views of one IR;
- **simulated** — the Rust interpreter runs small plans instantly;
- **compiled** — lowered to C, with tensor work delegated to ggml;
- **scheduled** — annotated with `@T(l.m.n)` time zones the editor renders as bands;
- **composed** — plans import other plans through `Portal`, like C functions from Python.

Two file kinds exist:

| Extension | Meaning |
|---|---|
| `.ufp` | Uncle Frank's Plan — graph, procedures, pipes, portals, annotations |
| `.txt` | Inert. Notes, marginalia, curses in plain prose. Never compiled, never imported |

`.txt` files are deliberately semantic no-ops. They exist so a project can carry
human context next to its plans without the toolchain pretending it understands it.

---

## 2. Plans, procedures, pins

A plan contains:

- `import` — builtin/utility import (`import_gguf`, tensor helpers, …);
- `portal` — a reference to another `.ufp` file, exposed as a node;
- `procedure` — a named function with typed pins;
- `node` — an instance of a procedure or portal inside a graph block;
- `pipe` — a directed edge between two pins;
- `@T(...)` / other annotations — scheduling and metadata.

### 2.1 Pins

A pin is an input, output, or inout of a procedure:

```ufp
procedure predict(
    input  matrix<float> y_pred with y_pred.shape = y_true.shape,
    input  matrix<float> y_true with y_true.shape = y_pred.shape,
    output matrix<float> out_pred with out_pred.shape = y_true.shape
) { ... }
```

Pin declarations come in three flavours:

1. **static** — the type is written and checked as-is;
2. **inferred** — the type is left as `auto` (or omitted) and derived from the body
   and from how the pin is wired;
3. **relational** — a written type plus `with` constraints over pin attributes
   (`shape`, `rank`, `dims`, `elem`, `length`, …). Constraints are solved during
   type checking and re-checked on every link in the editor.

Constraints may reference other pins of the same procedure, literals, and
`dim(i)`/`rank`/`elem` accessors. A constraint that cannot be solved is an error,
not a warning.

### 2.2 Pipes

```ufp
pipe dataset.x -> poison.x;
pipe poison.y -> loss.target;
```

A pipe is a typed channel. Fan-out is allowed (a value may feed many consumers);
fan-in on a single input pin is not — merge explicitly with a `select`/`merge` node.
Cycles are illegal in the plain dataflow graph. The only legal cycle is the
bounded fold described in §4.3.

---

## 3. Type system

### 3.1 Atomic and composite types

| Type | Meaning | Notes |
|---|---|---|
| `bool` | boolean | orderable `false < true` |
| `int` | machine integer, default width | |
| `int<32>` | sized integer, `int32_t` | width must be 8/16/32/64 |
| `float` | 32-bit float | |
| `double` | 64-bit float | |
| `string` | UTF-8 text | equality only, not orderable |
| `binary` | opaque byte blob | equality only |
| `list` / `list<T>` | mutable 1-D sequence | heterogeneous unless `T` given |
| `sheet` | 2-D table with named columns | column-wise typed, row read = `structured` |
| `matrix<E>[d0, d1, …]` | dense tensor, element type `E` | default `E = float` |
| `structured { … }` | named product type | dot-access, field-wise ops |
| `condition` | first-class predicate | `condition(<expr>)`; capture-checked |
| `auto` | inference placeholder | only on pins / `let` |

`matrix<3>[3,3]` is accepted for backward compatibility as a **rank assertion**
(`<3>` = expected rank). Canonical form is `matrix<float>[3,3]`. A rank assertion
that disagrees with the dimension list is a hard error. (See open question Q1.)

### 3.2 Indexing and field access

```ufp
m[0, 0]          // matrix element; index arity must equal rank
l[3]             // list element; bounds checked at runtime unless provable
s.name           // sheet column
x.matrix_var     // structured field
x.label
```

`structured` values are constructed with:

```ufp
let x = structured{ int index = 3, string label = "new llm", matrix_var };
```

Shorthand fields (`matrix_var`) adopt an existing binding and its type. Field
order is irrelevant; duplicate field names are an error. A `structured` value is
a first-class pipe payload, not a reference.

### 3.3 Comparison semantics

Comparisons are **element-wise and shape-preserving**:

- `matrix == matrix` → `matrix<bool>` of the same shape;
- `list == list` → `matrix<bool>[len]` (ragged lists are an error);
- `structured == structured` → `structured` with every field replaced by a
  `bool` (or `matrix<bool>`, …) field;
- ordering (`<`, `<=`, `>`, `>=`) requires every participating field to be
  orderable. Comparing a `string` field with `<` is an error; `==` is fine.
- scalar results are only produced by explicit reductions (`all`, `any`, `sum`,
  `mean`, `count`), never implicitly.

Because the runtime represents atomics as integers where possible, structured
comparisons lower to integer compares when the field type permits it (see Q4).
The guide rule: **if the label has no order, `<` is an error, `==` still works.**

### 3.4 `condition`

`condition` is a type for values that are predicates over the current scope:

```ufp
until (condition(. == qwen.END_TOKEN or tokens.length > 1000))
```

The compiler captures free variables explicitly, checks that every captured
binding is comparable with the operands, and refuses predicates with side
effects. A `condition` is not a `bool`; calling it (`c()`) yields `bool`.

---

## 4. Expressions and control flow

### 4.1 Operators

`+ - * / %`, comparisons, `and`/`or`/`not`, bitwise `& | ^ ~ << >>` for integers.
Matrix arithmetic is element-wise except `@`, which is matrix multiplication.
(`*` vs `@` is open question Q2; the spec currently follows NumPy.)

No implicit broadcasts except scalar-vs-matrix. Shape mismatches are errors with
both shapes printed.

### 4.2 Conditionals

```ufp
if (score > threshold) {
    out = activate(x);
} elif (score == threshold) {
    out = passthrough(x);
} else {
    out = zero_like(x);
}
```

Lowering: the condition becomes a predicate node; branches become regions; the
result is merged by a `phi`-style node that respects branch reachability. Both
branches must produce compatible types (same shape and element type after
constraint solving).

### 4.3 Iteration without loops

There are no general loops. Iteration is a bounded fold with an explicit
termination predicate:

```ufp
tokens = qwen.(tokens) run(
    token_tools.(tokens, .) concat(tokens)
    until (. == qwen.END_TOKEN or tokens.length > 1000)
);
```

Semantics:

- `.` is the **current accumulator**;
- `..` is the **enclosing procedure's result** (read-only; assigning it is an error);
- the expression between `run(` and `until` computes the next accumulator;
- the predicate is evaluated before each step; a true predicate stops the fold;
- the compiler inserts a **fuel bound** (default 10 000 iterations, overridable
  with `@fuel(...)`); exhausting fuel is a runtime error, not silent truncation;
- statically provable non-termination is a compile error.

### 4.4 Pipe-call sugar

`f.(a, b) g(c)` means: call `f(a, b)`, then call `g` with the result inserted as
its **first unbound argument**, i.e. `g(f(a, b), c)`. This is how Frank writes
"pass this along the pipe" without naming intermediates. The desugaring is
purely syntactic; the IR sees explicit edges. (Q7.)

---

## 5. Portals, imports, and nesting

```ufp
portal qwen = "./qwen3_4b.ufp" as Qwen_3_4b;
import_gguf("qwen3-4b.gguf") as weights;

node gen : Qwen_3_4b(prompt = tokens, weights = weights);
```

- A `portal` is a node whose pin signature is the exported procedure(s) of the
  target file. `as` names the imported unit; without it, the target's `main`
  procedure is used.
- `Portal` is the only way one `.ufp` consumes another. Unlike `import`, it is
  **inspectable**: clicking the node opens the target plan in a nested editor tab,
  with a breadcrumb back to the caller.
- Editing a portal target may change its signature. On save, the caller is
  re-checked; incompatible pins produce a migration diagnostic listing every
  affected pipe.
- Portal cycles (A → B → A) are legal to *edit* but illegal to *compile*;
  the editor renders them in red and offers a "break cycle" action.
- Each portal is cached by `content_hash(target.ufp) + compiler_version + flags`,
  so rebuilding an unchanged subtree is free.

`import` is for compiler-known utilities (`import_gguf`, tokenizers, tensor
helpers). It contributes no graph node unless the utility is stateful.

---

## 6. Static semantics

The checker runs in two passes:

1. **Type pass** — assign a type to every expression, pin, pipe, and node port.
   Inference is local except for `auto` pins, which require a fixpoint over the
   procedure body.
2. **Constraint pass** — collect `with` constraints, unify shape/rank/element
   relations, and reject unsolvable or contradictory systems. The solver is
   deterministic: same program ⇒ same error set, ordered by source span.

Errors carry: code (`UFP####`), span, message, and a hint where possible.
Warnings are reserved for things that are legal but suspicious (unused pipe,
unreachable branch, zone inversion, shadowed field).

Every diagnostic is available to the editor as structured JSON, and every
editor link operation reuses the same checker. The editor never invents its own
type rules.

---

## 7. Lowering to graph IR

Pipeline:

```
source ──lex/parse──▶ AST ──resolve──▶ module graph
      ──type/constraint──▶ typed AST
      ──desugar──▶ DFG (hierarchical nodes, typed ports, edges)
      ──schedule──▶ DFG + T zones
      ──validate──▶ IR
```

Desugaring rules:

| Source construct | IR |
|---|---|
| straight-line `let`/assign | edges + `bind` nodes |
| `if/elif/else` | `predicate` → `branch` regions → `merge` |
| `run … until` | `fold` node containing a body subgraph, `fuel`, `predicate` |
| `f.(a) g(b)` | edge from `f` output into `g`'s first free input |
| `structured{…}` | `make_struct` + field extraction nodes |
| indexing | `index` node with checked bounds |
| `Portal` | boundary node; target compiled separately |
| `@T(...)` | zone metadata on nodes/edges; phase barriers |

The IR is the single source of truth: the text formatter prints it, the editor
draws it, the compiler consumes it. Round-tripping editor → IR → editor must be
lossless for graph structure; comments and canvas positions live in side tables
keyed by stable ids.

---

## 8. Time zones — `T(l.m.n…)`

Zones are a **lexicographic schedule path**, not a decimal number:

- `@T(0)` — first phase;
- `@T(0.1)` — first child step of phase 0;
- `@T(0.1.2)` — third child of that step;
- siblings under one parent are **concurrent candidates**;
- a parent zone gates all of its children: children run after the parent's own
  nodes, and the parent does not finish until all children do.

`T(0)` and `T(1)` are sequential phases. `T(0.1)` and `T(0.2)` may run in
parallel. Nothing in the semantics *forces* parallel execution; zones describe
legal scheduling and are rendered as nested bands. The compiler may insert
barriers at zone boundaries when lowering to C.

Rules:

- dependency edges must not point "backwards" (producer zone > consumer zone is
  an error unless the edge crosses a declared `@async` boundary);
- duplicate/illegal paths are errors;
- un-annotated nodes inherit the **ASAP zone**: `1 + max(zone of dependencies)`
  at the same depth, so a clean plan gets a sensible schedule for free;
- `@concurrent` may be attached to a zone to assert that its children are
  deliberately parallel; the checker then also reports data races.

The editor renders zones as collapsible horizontal bands with a legend, a
per-zone status (clean / violated / unassigned), and a timeline view later.

---

## 9. Compilation to C and ggml

Stages:

1. **IR → C** — one C function per procedure, one `struct` per structured type,
   one handle type per `sheet`/`list`/`binary`.
2. **Partitioning** — subgraphs whose nodes are all matrix/tensor ops become a
   ggml graph; mixed scalar/control-flow stays in C. The partition boundary is
   materialised as tensor handles.
3. **ggml mapping** — `@` → `ggml_mul_mat`, `+`/`-` → `ggml_add`/`ggml_sub`,
   element-wise `*` → `ggml_mul`, transpose/reshape/softmax/rope via the matching
   ggml ops. Unsupported ops fall back to C (flagged in the build log).
4. **Fold codegen** — `run … until` becomes a `for` loop with the fuel counter
   and the predicate function; the body is inlined or called, never allocated
   per iteration.
5. **Portals** — each target `.ufp` compiles to its own translation unit with a
   generated header; the caller links against it. Cache is content-addressed.
6. **Build** — emit `.c` + runtime header, invoke `clang`/`cc`, link ggml
   (vendored by default), produce a shared library or executable plus a JSON
   manifest of artifacts and warnings.
7. **Runtime ABI** — arena-based allocation per invocation; slices are
   `{ptr, len, shape, elem}` views; ownership is explicit (`owned`, `borrowed`);
   no GC, no hidden copies. Strings/binary are length-prefixed.

`@T` zones become barriers in the generated C when `--schedule=strict`; in
`--schedule=relaxed` they are metadata only.

The compiler never accepts raw C, `asm`, or filesystem paths from source without
a capability grant. Builtins are a closed registry (§10).

---

## 10. Builtins and ML utilities

A closed, versioned registry. Each builtin declares typed pins and a capability
class (pure / alloc / io / model). Initial set:

| Builtin | Purpose |
|---|---|
| `import_gguf(path)` | load GGUF metadata and tensors as handles |
| `tokenizer.encode/decode` | text ↔ token lists |
| `matmul`, `transpose`, `reshape`, `softmax`, `layer_norm` | tensor primitives |
| `probe(value)` | attach an inspector; never alters data |
| `assert(condition, msg)` | runtime check |
| `len`, `all`, `any`, `sum`, `mean` | reductions |

Utilities are implemented in Rust for the interpreter and in C/ggml for native
builds; both must pass the same conformance vectors (§12).

---

## 11. Editor behaviour

The editor is a view over the IR, not a separate model:

- palette of node kinds, grouped (data / layer / loss / curse / control / portal);
- drag from palette, drag from output pin to input pin, drop to wire;
- a candidate link is validated with the same checker; incompatible links show
  the exact reason (type mismatch, shape constraint, zone inversion);
- invalid links are refused, never silently coerced;
- double-click a `Portal` → open target plan in a nested tab with breadcrumb;
- `@T` zones rendered as bands behind nodes, with a legend and per-zone status;
- inspector: pins, types, constraints, params, source span, zone;
- `.txt` files open in a plain notes pane and never enter the graph;
- compile/run panel streams diagnostics, build logs, and artifact paths;
- autosave to local storage; export/import project as JSON + source bundle.

The graph is a DAG by default. When a fold is expanded, its body is shown as a
nested region with a self-edge labelled `until`.

---

## 12. Testing and conformance

- Parser: golden files, span assertions, error-recovery tests, fuzz targets.
- Types: constraint-solver property tests (soundness: accepted programs type-check).
- IR: snapshot tests for every desugaring rule.
- Runtime: differential tests between the Rust interpreter and compiled C/ggml;
  tensor ops compared against reference vectors with documented tolerances.
- Editor: link-validation unit tests; end-to-end drag tests in a headless browser.
- Reproducibility: same plan + same seed + same compiler ⇒ identical artifacts
  (modulo build paths), verified by hash.

---

## 13. Worked example

```ufp
portal qwen = "./qwen3_4b.ufp" as Qwen_3_4b;

@T(0)    procedure generate(input string prompt, output list<int> tokens) {
    let seed = tokenizer.encode(prompt);
    tokens = seed run(
        token_tools.(tokens, .) concat(tokens)
        until (. == Qwen_3_4b.END_TOKEN or tokens.length > 1024)
    );
}
```

Lowered shape:

```
[T(0)] encode ──▶ fold(generate) ──▶ tokens
                     │  body: Qwen_3_4b.forward
                     │  pred: eq(., END) | gt(len(tokens), 1024)
                     └─ fuel: 1024
```

Note the deliberate restriction: the accumulator is `tokens`, the body appends,
and termination is a `condition` over the accumulator — no unbounded loop exists
in the IR.

---

## 14. Open questions (Frank must rule)

- **Q1** `matrix<3>[3,3]`: is `<3>` a rank, an element type, or legacy noise?
  Current reading: rank assertion; canonical `matrix<float>[3,3]`.
- **Q2** does `*` mean matmul or element-wise product for matrices? Current: NumPy
  rules (`*` element-wise, `@` matmul).
- **Q3** `sheet`: dataframe with typed columns? Does it need row mutation?
- **Q4** "all internal values are integers": are floats bit-cast to ints for
  comparison, or compared numerically? What does `structured{float a}` `<` mean?
- **Q5** may `string` fields be ordered lexicographically, or equality only?
  Current: equality only.
- **Q6** exact capture rules for `condition` (by value? by reference?).
- **Q7** is `f.(a) g(b)` sugar for "insert result as first free argument", or
  something else? This is the least certain syntax in the spec.
- **Q8** portal export naming: `main` by default, or require an `export` marker?
- **Q9** do T zones imply real parallelism (threads), or only legal scheduling?
  Current: legal scheduling + optional barriers in strict codegen.
- **Q10** default fuel for `run … until`, and whether `@fuel` is per-plan or
  per-fold.
- **Q11** `.txt` files: truly inert, or allowed as `@doc` attachments to nodes?
- **Q12** multiple entry points / multiple plans per project: how is the root
  chosen? (Current: `main` in the file opened in the editor.)
- **Q13** is `import` distinct from `portal` semantically, or only in UX?
- **Q14** scope of `..`: enclosing procedure's *return value* or its *caller*?
- **Q15** must ggml be a hard dependency, or is a pure-C fallback required?

---

## 15. Non-goals

- A general-purpose programming language (no loops, no heap objects, no closures
  beyond `condition`).
- Arbitrary C injection or `asm` in `.ufp`.
- A Python compatibility layer; `Portal` is composition, not interop with Python.
- Automatic differentiation of arbitrary user procedures (the toy runtime keeps
  its own hand-written backprop; the compiler targets inference first).
- Deploying anything outside the lab.

---

*"It's alive... and it's overfitting."*
