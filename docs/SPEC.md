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
- `input` / `output` — text terminals: the runtime trigger and the result sink,
  opened in the editor as text boxes (§2.3);
- `node` — an instance of a procedure or portal inside a graph block;
- `pipe` — a directed edge between two pins;
- `@T(...)` / other annotations — scheduling and metadata.

### 2.1 Pins

A pin is an input, output, or inout of a procedure:

```ufp
procedure predict(
    input  matrix<2>[200,2] y_pred with y_pred.shape = y_true.shape,
    input  matrix<2>[200,2] y_true with y_true.shape = y_pred.shape,
    output matrix<2>[200,2] out_pred with out_pred.shape = y_true.shape
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

### 2.3 Input and output terminals

`input` and `output` declare the plan's text terminals (decided, D-012). They are
ordinary procedures in the IR, not a new type constructor:

```ufp
input prompt = """
Uncle Frank, what is alive?
""";

output reply;

pipe prompt.text -> reply.text;
```

An `input` lowers to a zero-input procedure with one output pin, `text: string`.
It is both the **source** and the **trigger**: a run snapshots the text box and
fires every input terminal. An `output` lowers to a procedure with one input pin,
`text: string`, and no outputs; it is the **sink** and its box shows whatever
arrives.

Rules:

- input text is **string-only** in this version. There are no file, image, audio,
  or model source terminals yet;
- the input literal is required; `""` is a valid empty box;
- an output MUST receive exactly one incoming pipe on `reply.text`; an unwired
  output is a compile error (`UFP3404`);
- terminal names share the plan namespace and MUST be unique;
- the editor opens an input as an editable text box and an output as a read-only
  text box. Editing a box edits the literal in the source — there is no hidden
  editor-only state;
- multiple inputs and outputs are allowed. All inputs fire together unless a
  future `@trigger(...)` annotation narrows the set;
- text blocks (`"""…"""`) are raw: newlines are preserved and no escapes are
  processed. Ordinary strings use the escapes in §3 of the grammar notes.

---

## 3. Type system

### 3.1 Atomic and composite types

| Type | Meaning | Notes |
|---|---|---|
| `bool` | boolean | orderable `false < true` |
| `int` | machine integer, default width | |
| `int<32>` | sized integer, `int32_t` | width 8/16/32/64 |
| `float` / `float<32>` | IEEE float, 32-bit default | sized: `float<8>`, `float<16>`, `float<64>` |
| `float<8, e4m3>` / `float<8, e5m2>` | 8-bit float | variant required at 8 bits |
| `bfloat<16>` | bfloat16 | LLM-friendly exponent range |
| `double` | alias for `float<64>` | |
| `quant<q4_0>` … | ggml block-quantized element type | matrix-only; see below |
| `string` | UTF-8 text | equality only, not orderable |
| `binary` | opaque byte blob | equality only |
| `list` / `list<T>` | mutable 1-D sequence | heterogeneous unless `T` given |
| `sheet` | 2-D table with named columns | column-wise typed, row read = `structured` |
| `matrix<r>[d0, …, d(r-1)]` | dense tensor, rank `r`, element type from `of` | rank optional; elements default to `float` |
| `structured { … }` | named product type | dot-access, field-wise ops |
| `condition` | first-class predicate | `condition(<expr>)`; capture-checked |
| `auto` | inference placeholder | only on pins / `let` |

**Matrix syntax (decided, Q1 → D-006):** `<r>` is the **rank**, not the element
type, and the bracket list gives the extents:

```ufp
matrix<3>[3,3,3]                    // rank 3, 3×3×3, float elements
matrix<2>[200,2] of double           // rank 2, 200×2, float<64>
matrix<2>[4,4] of int<32>            // 32-bit integer matrix
matrix<2>[128,64] of float<16>       // half precision
matrix<2>[4096,4096] of quant<q4_0>  // ggml block-quantized weights
matrix[3,3,3]                        // rank omitted; inferred from the dims
```

When the rank is written it must equal the number of dimensions — a mismatch is a
hard error. The element type is written with `of` (default `float`). The old
`matrix<float>[...]` spelling is gone.

**Precision and quantization (decided, D-009).** Frank's LLMs may carry whatever
precision they like:

- `float<8|16|32|64>`; at 8 bits the variant is explicit (`e4m3`, `e5m2`);
  `bfloat<16>` is a distinct type, not a `float<16>` alias.
- `quant<fmt>` is a **matrix-only** element type, using ggml's block formats
  (`q4_0`, `q4_K`, `q5_K`, `q6_K`, `q8_0`, `iq2_xxs`, …). The format registry is
  versioned with the compiler; unknown names are errors.
- A quantized matrix stores blocks + scales. Its extents must satisfy the
  format's block size (e.g. q4_0 needs the contraction dimension to be a multiple
  of 32); the checker enforces this.
- `*` (matmul) is allowed with float activations: `float<32> * quant<q4_0>`
  lowers to `ggml_mul_mat`. `quant * quant`, element-wise arithmetic, and element
  indexing are rejected until the value is made explicit:
  `dequantize(w)`, or `quantize(m, q4_0)`.
- Accumulation is `float<32>` regardless of storage precision, matching ggml.
- Matrix literals cannot be quantized; quantization is a runtime/storage step,
  never a source literal.

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

**Scope walking (decided, D-011).** Every `structured` literal — and every
procedure body — opens a lexical scope. Dots walk outward, one per level:

```ufp
.field          // self
..field         // parent scope
...field        // grandparent scope
..matrices[0]   // parent's `matrices`, first substructure
```

`.` is self, `..` is the immediately enclosing scope, `...` the one outside
that, and so on. Reads from outer scopes are allowed; writing through `..` is an
error (use an `inout` pin instead). Resolution is static: the checker rewrites
each scope path to a concrete binding, so there is no runtime lookup and no
dynamic scoping.

### 3.3 Comparison semantics

Comparisons are **element-wise and shape-preserving**:

- `matrix == matrix` → a `bool` matrix of the same shape;
- `list == list` → `list<bool>` (ragged lists are an error);
- `structured == structured` → `structured` with every field replaced by a
  `bool` (or bool-matrix, …) field;
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

Matrix semantics are **ggml semantics** (decided, Q2 → D-007):

| Source | Meaning | ggml lowering |
|---|---|---|
| `a * b` | matrix multiply | `ggml_mul_mat` (with transposes so source stays row-major `a·b`) |
| `a .* b` | element-wise (Hadamard) | `ggml_mul` |
| `a / b` | element-wise division | `ggml_div` |
| `a + b`, `a - b` | element-wise | `ggml_add`, `ggml_sub` |
| `-a` | negation | `ggml_neg` |
| `transpose(a)` | swap last two dims | `ggml_transpose` |

`*` is matmul; element-wise multiply is `.*`. There is no `@` — one spelling per
operation. Contracting dimensions must agree: `[m,k] * [k,n] → [m,n]`; batched
ranks follow ggml's leading-dim rules, and a mismatch reports both shapes.
A scalar times a matrix is scalar scaling (fused by the compiler); that is the
only implicit broadcast.

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

There are no general loops. Iteration is a **conditional feedback loop** — a
*do-until*: the body procedure is called, and its result is passed back as the
next input **only while the `until` condition is unmet** (decided, D-010).

```ufp
tokens = qwen.(tokens) run(
    token_tools.(tokens, .) concat(tokens)
    until (. == qwen.END_TOKEN or tokens.length > 1000)
);
```

Semantics:

1. `acc ← seed` (the expression before `run`).
2. Call the body with `acc`; `.` is `acc`. The body's outputs become `next`.
3. Evaluate `until (condition)` against `next`.
4. **Condition met** → stop, yield `next`; the feedback edge is *not* taken.
5. **Condition unmet** → pass `next` back as the next `acc` and call again.

Consequences:

- the body runs **at least once** — this is do-until, not while;
- the loop-carried back-edge is conditional: tokens are passed back only on the
  unmet branch, which is exactly what the editor draws as a gated self-edge;
- `.` is the current accumulator; `..` / `...` walk to enclosing scopes (§3.2),
  so a fold body may read outer bindings but never assign them;
- the compiler inserts a **fuel bound** (default 10 000 iterations, overridable
  with `@fuel(...)`); fuel exhaustion is a runtime error, not silent truncation;
- statically provable non-termination is a compile error.

### 4.4 Pipe chains — Verilog-style ports

Think of the chain as Verilog module instantiation: **inputs are fed from the
left, outputs exit to the right.**

```ufp
token_tools.(tokens, .) concat(tokens)
```

desugars to:

```verilog
// Verilog intuition
concat u_concat(.in0(token_tools(tokens, .)), .in1(tokens));
// outputs exit right, into the assignment / next link
```

Rules (decided, Q7 → D-008):

1. `callee.(a, b)` is an **input port map**: call `callee` binding its input pins
   by position (or `name = value`). `.` is the current fold accumulator.
2. A following `next(c, …)` is the next module in the chain. Every output of the
   previous link is wired, left to right, into `next`'s **leftmost unbound input
   pins**. Remaining arguments fill subsequent input pins in order.
3. `next`'s outputs are never named on the left; they exit to the right and become
   the value of the expression (or feed the next link).
4. The named form is authoritative and always legal:
   `concat(in0 = token_tools(tokens, .), in1 = tokens)`.

The sugar is removed during lowering; the IR only sees explicit edges. The
editor always shows the fully named pin form.

> The one remaining reading risk is whether `g(x)` after a link was meant as
> *input* binding (current reading) or output naming; see D-008. Named ports make
> the ambiguity disappear in real code.

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
| `run … until` | `fold` node: body subgraph, predicate, **conditional feedback edge** (do-until), `fuel` |
| `f.(a) g(b)` | outputs of `f` wired into `g`'s leftmost free inputs (Verilog-style ports) |
| `structured{…}` | `make_struct` + field extraction nodes |
| indexing | `index` node with checked bounds |
| `.` / `..` / `...` | static resolution to a binding; no runtime lookup |
| `Portal` | boundary node; target compiled separately |
| `input name = "…"` | source procedure: 0 inputs, `text: string` output; run trigger |
| `output name` | sink procedure: `text: string` input, 0 outputs; captured result |
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
3. **ggml mapping** — `*` → `ggml_mul_mat`, `.*` → `ggml_mul`, `+`/`-` →
   `ggml_add`/`ggml_sub`, `/` → `ggml_div`; transpose/reshape/softmax/rope use
   matching ggml ops. Unsupported ops fall back to C (flagged in the build log).
4. **Fold codegen** — `run … until` becomes a do-until loop: body first, then the
   predicate; the feedback edge is copied back only on the unmet branch. Fuel is a
   counter, the body is inlined or called, and nothing is allocated per iteration.
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
| `import_gguf(path)` | load a GGUF as a nested `structured` (metadata + matrix substructures) |
| `tokenizer.encode/decode` | text ↔ token lists |
| `matmul`, `transpose`, `reshape`, `softmax`, `layer_norm` | tensor primitives |
| `probe(value)` | attach an inspector; never alters data |
| `assert(condition, msg)` | runtime check |
| `len`, `all`, `any`, `sum`, `mean` | reductions |

Utilities are implemented in Rust for the interpreter and in C/ggml for native
builds; both must pass the same conformance vectors (§12).

### 10.1 A loaded GGUF is plain nested structure

No model type and no new type constructors are introduced. `import_gguf` returns
an ordinary `structured`; metadata is a substructure and the matrix set is a
heterogeneous `list` of tensor substructures:

```ufp
let model = import_gguf("qwen3-4b-q4_0.gguf");

// model.metadata.architecture
// model.matrices[0].name
// model.matrices[0].data
```

Canonical layout:

```
structured {
  string path,
  string version,
  structured {                       // metadata
    string architecture,
    int    n_layers,
    int    n_embd,
    int    n_heads,
    int    n_vocab
  } metadata,
  list matrices                      // one substructure per tensor
}
```

Each entry of `matrices` is itself a `structured`:

```
structured {
  string   name,
  list<int> shape,               // e.g. [4096, 4096]
  string   dtype,                // ggml format name, e.g. "q4_0"
  matrix<2>[4096,4096] of quant<q4_0> data   // this tensor's specialised type
}
```

Because the GGUF is a compile-time input, the loader **specialises this
structure from the file's metadata**: the rank `r`, the extents and the element
type are substituted per tensor, so `model.matrices[3].data` is a concrete
`matrix<r>[d0, …, d(r-1)] of <elem>` — never a dynamically typed matrix. A
constant index (`matrices[0]`) sees that specialised type; a non-constant index
sees only the common fields (`name`, `shape`, `dtype`) and cannot touch `.data`.
No new type constructor is added; "dynamic" indexing is simply rejected until the
index is constant.

Inside a structure, the parent's matrix set is reached with `..`:

```ufp
let block = structured {
    list matrices = ..model.matrices,        // procedure scope
    structured {
        string name = ..matrices[0].name,    // outer structure scope
        auto   w    = ..matrices[0].data
    } head
};
```

`..matrices[0]` is ordinary scope walking (§3.2), not a GGUF-specific rule.

---

## 11. Editor behaviour

The editor is a view over the IR, not a separate model:

- palette of node kinds, grouped (data / layer / loss / curse / control / portal);
- drag from palette, drag from output pin to input pin, drop to wire;
- a candidate link is validated with the same checker; incompatible links show
  the exact reason (type mismatch, shape constraint, zone inversion);
- invalid links are refused, never silently coerced;
- double-click a `Portal` → open target plan in a nested tab with breadcrumb;
- double-click an `input`/`output` → open its text box; inputs are editable and
  act as the run trigger, outputs are read-only result boxes;
- `@T` zones rendered as bands behind nodes, with a legend and per-zone status;
- inspector: pins, types, constraints, params, source span, zone;
- `.txt` files open in a plain notes pane and never enter the graph;
- compile/run panel streams diagnostics, build logs, and artifact paths;
- clicking Run opens/focuses an interactive terminal session: editable input
  text boxes, read-only output boxes, and a Run-with-these-values action;
- the canvas highlights the current coarse execution block. Normal mode emits
  bounded node/block events and never copies tensors or blocks execution for
  rendering; an optional speed control delays only between events (D-013);
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
input prompt = "Why did the tensor cross the road?";

output story;

pipe generate.tokens -> story.text;   // once token decoding is wired

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
                     │  body: token_tools + concat
                     │  pred: eq(., END) | gt(len(tokens), 1024)
                     └─ fuel: 1024
```

Note the deliberate restriction: the accumulator is `tokens`, the body appends,
and termination is a `condition` over the accumulator. The back-edge is gated on
"condition unmet"; no unbounded loop exists in the IR.

---

## 14. Open questions (Frank must rule)

Resolved: **Q1** → D-006 (matrix rank), **Q2** → D-007 (`*` = matmul, `.*` = Hadamard),
**Q7** → D-008 (Verilog-style port chains), **Q14** → D-011 (scope walking),
**I/O terminals** → D-012 (`input` is the text trigger, `output` the text sink).

- **Q3** `sheet`: dataframe with typed columns? Does it need row mutation?
- **Q4** "all internal values are integers": are floats bit-cast to ints for
  comparison, or compared numerically? What does `structured{float a}` `<` mean?
- **Q5** may `string` fields be ordered lexicographically, or equality only?
  Current: equality only.
- **Q6** exact capture rules for `condition` (by value? by reference?).
- **Q8** portal export naming: `main` by default, or require an `export` marker?
- **Q9** do T zones imply real parallelism (threads), or only legal scheduling?
  Current: legal scheduling + optional barriers in strict codegen.
- **Q10** default fuel for `run … until`, and whether `@fuel` is per-plan or
  per-fold.
- **Q11** `.txt` files: truly inert, or allowed as `@doc` attachments to nodes?
- **Q12** multiple entry points / multiple plans per project: how is the root
  chosen? (Current: `main` in the file opened in the editor.)
- **Q13** is `import` distinct from `portal` semantically, or only in UX?
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
