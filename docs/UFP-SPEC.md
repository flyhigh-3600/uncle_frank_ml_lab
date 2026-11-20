# Uncle Frank's Plan — Language Specification

**Document:** UFP-SPEC-1 · **Version:** 1.0.0-draft · **Status:** normative draft
**Grammar:** [`grammar.ebnf`](./grammar.ebnf) (normative syntax)
**Design overview:** [`SPEC.md`](./SPEC.md) · **Decisions:** [`DECISIONS.md`](./DECISIONS.md)

This document is the normative definition of the `.ufp` language and its runtime
contracts. Where this document and the design overview disagree, this document
wins; where this document and the grammar disagree about syntax, the grammar
wins. Unresolved semantics are marked **OPEN** and reference the decision log.

Conformance keywords: **MUST**, **MUST NOT**, **SHOULD**, **MAY**.

---

## 1. Scope and conformance

A conforming implementation provides:

1. a **lexer and parser** accepting the grammar in §3;
2. a **static checker** enforcing §4–§8 and rejecting every program that violates
   a MUST-level rule;
3. a **lowering** to the IR of §11;
4. an **interpreter** or **native backend** implementing §14–§15;
5. a **diagnostic** channel in the format of §16.

Conformance classes:

| Class | Requires |
|---|---|
| `syntax` | lexer + parser + diagnostics |
| `static` | syntax + type/constraint checking + IR validation |
| `runtime` | static + interpreter for the supported builtin set |
| `native` | runtime + C codegen + ggml partition |

An implementation MUST NOT accept a program it cannot execute unless the
unsupported construct is rejected at check time with a diagnostic.

---

## 2. Lexical structure

### 2.1 Source

- Encoding MUST be UTF-8. A leading BOM is ignored.
- Line endings may be LF or CRLF.
- A source file has the extension `.ufp`. Files with extension `.txt` are never
  lexed or compiled (D-001).

### 2.2 Whitespace and comments

Whitespace separates tokens but is otherwise insignificant.

```
line_comment  = "//" { any char except newline } newline?
block_comment = "/*" { any char } "*/"
```

Block comments do not nest. An unterminated block comment is a lexical error
(`UFP1002`). Comments are trivia: they do not produce tokens, but an editor MUST
preserve them in its side table (§17).

### 2.3 Identifiers and keywords

```
ident = ( "A".."Z" | "a".."z" | "_" ) { "A".."Z" | "a".."z" | "0".."9" | "_" }
```

Identifiers are ASCII in this version. Unicode identifiers are reserved for a
future revision.

Reserved words (MUST NOT be used as identifiers):

```
procedure node pipe portal import text graph
input output inout with and or not
if elif else return let run until
structured condition matrix list sheet
bool int float bfloat quant double
string binary true false null auto as from of
```

`T` is **not** reserved; it is an ordinary identifier used as an annotation name
(`@T(...)`).

### 2.4 Literals

| Literal | Form | Notes |
|---|---|---|
| integer | `0`, `123`, `0xFF` | decimal or hexadecimal |
| float | `1.0`, `0.5`, `1e-3` | decimal point or exponent required |
| string | `"..."` | UTF-8, escapes below |
| text block | `"""..."""` | raw UTF-8, newlines preserved, no escapes |
| boolean | `true`, `false` | |
| null | `null` | absence; only for `auto`-typed contexts |

String escapes: `\"`, `\\`, `\n`, `\r`, `\t`, `\0`, `\u{1F600}`. An unknown
escape is an error (`UFP1005`). A raw newline inside a quoted string is an error.
Triple-quoted text blocks are raw: every byte between the delimiters is content,
newlines included, and backslashes are ordinary characters. An unterminated text
block is `UFP1006`.

A digit sequence immediately followed by `.` and a digit is a **float**. In zone
annotations (`@T(...)`) floats are disabled; each numeric segment lexes as an
integer and each dot as a separate `DOT` token, so `@T(0.1.2)` is
`INT(0) DOT INT(1) DOT INT(2)`. This is the only context-sensitive lexing rule.

### 2.5 Punctuation and operators

```
( ) { } [ ]  , ; :  .  ->  @
+ - * / %  .*  == != <= >= < >  =  & | ^ ~ << >>
```

`.*` MUST be lexed as a single token. A run of dots always lexes as individual
`DOT` tokens; the parser groups them into scope depth (§5). `->` MUST be a
single token. `!` alone is not an operator; negation is `not`.

### 2.6 Spans

Every token carries a half-open byte span `[start, end)`, a 1-based line, and a
1-based column. Diagnostics use the same coordinates.

---

## 3. Grammar

The normative grammar is [`grammar.ebnf`](./grammar.ebnf). Its production index:

```
plan, top_level, import_decl, portal_decl, text_decl, io_decl, input_decl,
output_decl, procedure_decl, pin_list, pin, graph_block, node_decl, pipe_decl,
block, statements, expressions, postfix, scoped_ident, types, constraints,
annotations
```

Notation: `[x]` optional, `{x}` zero or more, `|` alternative, `"x"` terminal.
The grammar is LL(1) except for the dot-run lookahead described in §5.2 and the
`matrix<r>` disambiguation, which is resolved by the parser as follows:

- `matrix <` followed by an integer is a rank;
- `matrix <` followed by a type name is rejected (element type uses `of`).

---

## 4. Types

### 4.1 Closed type set

The type constructors are closed. Implementations MUST NOT add new constructors,
and a loaded GGUF (D-011) introduces none: it is a `structured` composed of these
existing types.

| Type | Meaning | Default / notes |
|---|---|---|
| `bool` | boolean | orderable `false < true` |
| `int` | signed integer | alias of `int<32>` |
| `int<n>` | signed integer, `n ∈ {8,16,32,64}` | two's complement |
| `float` | IEEE-754 binary floating point | alias of `float<32>` |
| `float<n>` | `n ∈ {8,16,32,64}` | 8-bit requires a variant |
| `float<8, e4m3>`, `float<8, e5m2>` | 8-bit float variants | |
| `bfloat<16>` | bfloat16 | distinct from `float<16>` |
| `double` | alias of `float<64>` | |
| `quant<fmt>` | block-quantized element | matrix-only; registry §13.3 |
| `string` | immutable UTF-8 text | equality only |
| `binary` | immutable byte blob | equality only |
| `list` | mutable heterogeneous 1-D sequence | element type optional |
| `list<T>` | homogeneous list | |
| `sheet` | 2-D table with named columns | **provisional** (Q3) |
| `matrix<r>[d0,…,d(r-1)]` | dense r-dimensional tensor | rank optional |
| `matrix<r>[d…] of T` | tensor with element type `T` | default `float` |
| `structured { … }` | named product | fields, dot access |
| `condition` | first-class predicate | capture-checked (Q6) |
| `auto` | inference placeholder | pins and `let` only |

No implicit numeric coercions exist except:

- `int<n>` → `int<m>` where `m > n` (widening), and
- `float<16>`/`bfloat<16>` → `float<32>`/`float<64>` in arithmetic.

Narrowing and float↔int conversions MUST be explicit (`cast<T>(x)`).

### 4.2 Matrix types

```
matrix<3>[3,3,3]                       rank 3, extents 3×3×3, float
matrix<2>[200,2] of double             float<64> elements
matrix<2>[4096,4096] of quant<q4_0>    block-quantized weights
matrix[3,3,3]                          rank inferred from extents
```

- `<r>` is the **rank**; if present it MUST equal the number of extents.
- Extents are constant expressions or symbolic names resolved by the constraint
  system.
- `matrix` with neither rank nor extents denotes a runtime-shaped value and is
  only legal where the checker can specialise it (GGUF constant indices, §13.2).
- Indexing `m[i0,…,i(r-1)]` MUST supply exactly `r` indices. Out-of-bounds is a
  runtime error unless statically provable, in which case it is a compile error.
- A matrix is homogeneous; mixed element types require an explicit conversion.

### 4.3 Quantized elements

- `quant<fmt>` is legal only as a matrix element type.
- `fmt` MUST name a registered ggml block format; unknown names are errors.
- Extents MUST satisfy the format's block constraint (e.g. q4_0 requires the
  contraction extent to be a multiple of 32).
- `*` with float activations is legal and accumulates in `float<32>`:
  `float<32> * quant<q4_0>` lowers to `ggml_mul_mat`.
- `quant * quant`, element-wise arithmetic on `quant`, and indexing a `quant`
  matrix are type errors. Use `dequantize(m)` or `quantize(m, fmt)` explicitly.
- Matrix literals MUST NOT be quantized.

### 4.4 Lists, sheets, structures

- `list` is mutable, one-dimensional, and may be heterogeneous. `list<T>` fixes
  the element type. `list.length` is an integer property; indexing is `l[i]`.
- `sheet` is a 2-D table with named, typed columns. A row read yields a
  `structured`; column access is `s.name`; row/column mutation is **OPEN** (Q3).
- `structured { T f = init, … }` declares a product. Fields may be shorthand
  (`f` adopts the binding `f`). Field names are unique; order is irrelevant.
- A `structured` is a value, not a reference. Assignment copies (or shares
  immutably) the fields; there are no aliasing guarantees at the language level.

### 4.5 `condition`

`condition` is a predicate value over captured bindings.

- `condition(expr)` constructs it; `expr` MUST be `bool`-valued.
- Free variables are captured explicitly by the checker and reported in
  diagnostics. Capture is by value; mutation after capture is not visible (Q6).
- A `condition` is not a `bool`; applying it (`c()`) yields `bool`.
- Captured operands MUST be comparable or equality-testable as required by the
  expression; a `condition` with side effects is rejected.

### 4.6 Equality and ordering

- `==` / `!=` are defined for all types; on matrices and lists they are
  element-wise and shape-preserving, yielding a bool matrix/list.
- Ordering (`< <= > >=`) is defined for `bool`, integers, floats, and
  field-wise for `structured`/`sheet` when every field is orderable.
- `string` and `binary` support equality only (Q5). Ordering a structure with a
  non-orderable field is an error.
- `quant` matrices may be compared only after `dequantize`.
- Reductions (`all`, `any`, `sum`, `mean`, `count`) are the only implicit way to
  turn an element-wise comparison into a scalar.

---

## 5. Names, scopes, and the dot operators

### 5.1 Scopes

The following open lexical scopes:

1. a plan (top level),
2. a procedure body,
3. a `structured` literal,
4. a fold body (the `run(… until …)` expression).

Names resolve lexically: current scope first, then enclosing scopes.

### 5.2 Dot forms

| Form | Meaning |
|---|---|
| `.` | self: the current value/accumulator, or the enclosing structure |
| `.f` | field `f` of self |
| `..f` | field `f` of the parent scope |
| `...f` | field `f` of the grandparent scope |
| `..matrices[0]` | parent scope's `matrices`, first element |
| `x.f` | field access on value `x` (postfix) |
| `f.(a, b)` | input port map of `f` (§7.6) |

A run of `k` dots followed by an identifier denotes scope depth `k` (1 = self).
The lexer emits one `DOT` per dot; the parser consumes the maximal run.

### 5.3 Rules

- Outward reads are allowed; writes through `..` or `...` are errors.
- Shadowing is allowed; the nearest binding wins. The checker SHOULD warn on
  shadowing a procedure parameter.
- A scope path MUST resolve statically; there is no runtime lookup and no
  dynamic scoping.
- A field of a `structured` is visible inside its own literal only via `.f` or
  the shorthand `f` when in scope.

---

## 6. Procedures, pins, and constraints

### 6.1 Declarations

```
procedure name(input T a, output U b, inout V c) { … }
```

- A procedure has zero or more pins. Pins are `input`, `output`, or `inout`.
- A procedure MAY have a `graph { … }` body instead of a statement block; both
  lower to the same IR.
- The exported procedure is `main` unless a portal selects another by name
  (Q8, **OPEN**).

### 6.2 Pin types

A pin type is one of:

1. **static** — `input matrix<2>[200,2] x`;
2. **inferred** — `input auto x`; the checker solves it from the body and the
   wiring, using a least-fixed-point; failure is an error;
3. **relational** — a static type plus `with` constraints:

```
procedure predict(
    input  matrix<2>[200,2] y_pred with y_pred.shape = y_true.shape,
    input  matrix<2>[200,2] y_true with y_true.shape = y_pred.shape,
    output matrix<2>[200,2] out_pred with out_pred.shape = y_true.shape
) { … }
```

### 6.3 Constraint language

A constraint is `expr cmp_op expr` over:

- pin attributes: `.shape` (list of extents), `.rank`, `.dims`, `.length`,
  `.elem` (element type), `.rows`, `.cols`;
- integer/float literals and arithmetic;
- scope paths and constant names.

Constraint semantics:

- `=` is unification, not assignment.
- The solver collects all constraints, propagates equalities, and reports the
  minimal contradictory set.
- A solved shape MUST be positive in every extent, except rank-0/scalars.
- Constraints are re-checked at every editor link; the checker is authoritative.

### 6.4 Input and output terminals

`input` and `output` are text terminals (D-012). They do not add types or
constructors; they lower to ordinary procedures:

```ufp
input prompt = """
Uncle Frank, what is alive?
""";

output reply;

pipe prompt.text -> reply.text;
```

| Declaration | Lowers to |
|---|---|
| `input name = <text>;` | procedure with 0 inputs and one output `text: string` |
| `output name;` | procedure with one input `text: string` and 0 outputs |

- An input's text is **string-only** in this version. The literal is required;
  `""` is a legal empty box.
- Every input is a **run trigger**: starting the plan snapshots all input boxes
  and fires every input terminal.
- An output is a **sink**: its `text` pin MUST be bound by exactly one pipe.
  An unbound output is `UFP3404`.
- Terminal names share the plan namespace; duplicates are `UFP3403`.
- A pipe touching a terminal MUST use the `text` pin; other endpoint names are
  `UFP3405`.
- The editor opens an input as an editable text box and an output as a read-only
  text box. Editing a box rewrites the source literal, preserving the single
  IR/source-of-truth rule.

---

## 7. Expressions

### 7.1 Precedence

From lowest to highest:

| Level | Operators | Associativity |
|---|---|---|
| 1 | `or` | left |
| 2 | `and` | left |
| 3 | `== != < <= > >=` | non-chaining |
| 4 | `+ -` | left |
| 5 | `* / % .*` | left |
| 6 | unary `- not ~` | prefix |
| 7 | postfix `. [] ()` | left |

Comparisons do not chain; `a < b < c` is a syntax error. Use `and`.

### 7.2 Arithmetic

- `+ - * /` follow IEEE-754 for floats.
- `/` on integers truncates toward zero; division by zero is a runtime error.
- `%` is defined for integers only; sign follows the dividend.
- Overflow wraps for sized integers; the checker MAY warn.
- NaN compares false with everything, including itself; the spec does not
  provide a total-order comparison except in reductions (Q4, **OPEN**).

### 7.3 Matrix operators

| Source | Meaning | ggml |
|---|---|---|
| `a * b` | matmul, contraction over the last extent of `a` and first of `b` | `ggml_mul_mat` |
| `a .* b` | element-wise | `ggml_mul` |
| `a / b` | element-wise | `ggml_div` |
| `a + b`, `a - b` | element-wise | `ggml_add`, `ggml_sub` |
| `-a` | negation | `ggml_neg` |
| `transpose(a)` | swap the last two extents | `ggml_transpose` |

- `[m,k] * [k,n] → [m,n]`. Batched ranks follow ggml's leading-extent rules.
- A scalar MAY be combined with a matrix; this is the only implicit broadcast.
- A shape mismatch is an error that MUST print both shapes.

### 7.4 Indexing and fields

- `m[i0,…,i(r-1)]` requires exactly `r` indices.
- `l[i]` requires a list; `l.length` is an integer.
- `x.f` requires `x` to be a `structured` or `sheet`; unknown fields are errors.
- `s.name` on a sheet yields the column.

### 7.5 Calls and port maps

- `f(a, b)` is a call with positional arguments.
- `f.(a, b)` is an **input port map**: bind `f`'s input pins by position or by
  `name = value`.
- Output selection: results exit to the right; a procedure cannot name its
  outputs on the left of a chain.

### 7.6 Port chains (Verilog-style)

`f.(a, b) g(c)` desugars to:

1. invoke `f` with inputs `a, b`;
2. wire every output of `f`, left to right, into `g`'s leftmost unbound inputs;
3. fill any remaining `g` inputs from `c, …` in order;
4. the value of the expression is `g`'s outputs, which exit right.

The named form is authoritative and always legal:

```
g(in0 = f(a, b), in1 = c)
```

If the number of outputs of `f` plus the supplied arguments exceeds `g`'s input
arity, the program is an error.

---

## 8. Statements and control flow

### 8.1 Statements

```
let x = expr;  |  x = expr;  |  if …  |  return expr;  |  expr;
```

- `let` binds a new name in the current scope; `=` assigns to an existing
  binding. Assignment to an input pin is an error unless it is `inout`.
- A procedure returns its output pins; `return` may also yield a value when the
  procedure has exactly one output.

### 8.2 Conditionals

`if (c) { … } elif (c2) { … } else { … }`.

- `c` MUST be `bool`.
- Both branches MUST produce compatible types for every live binding.
- Lowering creates a predicate node, branch regions, and a merge/phi node.

### 8.3 Iteration: do-until

There are no general loops. The only iteration form is:

```
result = seed run( body until (condition) )
```

Semantics, in order:

1. `acc ← seed`.
2. Call `body` with `acc` bound to `.`; let `next` be its result.
3. Evaluate `condition(next)`.
4. If **true**, stop and yield `next`. The feedback edge is not taken.
5. If **false**, pass `next` back as the next `acc` and repeat.

- The body runs at least once (do-until, not while).
- The feedback edge exists only on the unmet branch.
- `..` inside the body is the parent scope, not the previous result (§5).
- Fuel: default 10 000 iterations; `@fuel(n)` overrides per fold. Exhaustion is
  a runtime error (`UFP5003`). A statically provable non-terminating fold is a
  compile error.
- The accumulator type MUST be stable across iterations.

---

## 9. Plans, pipes, portals, imports

### 9.1 Plan items

A plan MAY contain `import`, `portal`, `text`, `input`, `output`, `procedure`,
`node`, `pipe`, and annotations. Top-level `node`/`pipe` declarations and I/O
terminals form an implicit `main` plan.

### 9.2 Pipes

```
pipe dataset.x -> poison.x;
```

- A pipe connects an output pin to an input pin; both ends MUST exist.
- Fan-out is allowed. An input pin MUST have at most one incoming pipe.
- Cycles are illegal in the plain dataflow graph; the fold is the only legal
  cycle (§8.3).
- Type and constraint agreement is checked at wiring time.

### 9.3 Nodes

```
node encode : tokenizer.encode(text = prompt);
```

A node instantiates a procedure, portal, or builtin. Bindings fill input pins.
Unbound inputs MAY be filled by pipes. All inputs MUST be bound before execution.

### 9.4 Portals

```
portal qwen = "./qwen3_4b.ufp" as Qwen_3_4b;
```

- A portal exposes the target's exported procedure under `as` (default `main`).
- The pin signature is derived from the target and cached.
- Clicking a portal opens the target plan; editing the target re-checks callers.
- Portal cycles are legal to edit but illegal to compile; the check reports the
  full cycle path.
- Cache key = `content_hash(target.ufp) + compiler_version + flags`.

### 9.5 Imports and text

- `import` names compiler-known builtins (`import_gguf`, tokenizers, tensor
  utilities); it contributes no user graph unless the builtin is stateful.
- `text name = "notes.txt";` attaches an inert `.txt`. It is never lexed or
  compiled and MAY be shown in the editor notes pane (Q11, **OPEN**).

### 9.6 Entry point

If a plan declares `main`, that is the entry. Otherwise the file opened by the
editor is the root (Q12, **OPEN**).

---

## 10. Static semantics

### 10.1 Checker order

1. **Resolve** imports, portals, and module graph.
2. **Bind** names and scopes (§5).
3. **Infer** `auto` pins and `let` types to a fixed point.
4. **Constrain** shapes, ranks, elements, and lengths; solve.
5. **Check** all expressions, pipes, nodes, and control flow.
6. **Lower** to IR (§11) and validate invariants.

The checker MUST be deterministic: the same source yields the same diagnostics
in source order, regardless of hash-map iteration order.

### 10.2 Type rules (selected)

```
Γ ⊢ e : matrix<r>[d…] of T      Γ ⊢ i0..i(r-1) : int
──────────────────────────────────────────────────────
Γ ⊢ e[i0,…,i(r-1)] : T

Γ ⊢ a : matrix<r>[d…] of T      Γ ⊢ b : matrix<r>[d…] of T
──────────────────────────────────────────────────────
Γ ⊢ a .* b : matrix<r>[d…] of T

Γ ⊢ a : matrix<2>[m,k] of Ta    Γ ⊢ b : matrix<2>[k,n] of Tb
──────────────────────────────────────────────────────
Γ ⊢ a * b : matrix<2>[m,n] of float<32>
```

(The full rule set is implemented by the checker; this table is informative.)

### 10.3 `auto` inference

- `auto` starts as an unknown and is unified by assignments, calls, pipes, and
  constraints.
- If the fixed point leaves an unknown, the declaration is an error
  (`UFP3005`) naming the unresolved pin.
- Inference never widens `float<16>` to `float<32>` silently; the program MUST
  make precision explicit when it matters.

### 10.4 Quantized constraints

- The block size divides the contraction extent; otherwise `UFP3204`.
- A `quant` value MUST NOT reach an operation that requires a scalar element
  (indexing, comparison, element-wise math) without `dequantize`.

---

## 11. Intermediate representation

### 11.1 Schema

The IR is a hierarchical dataflow graph.

| Entity | Fields |
|---|---|
| `Program` | modules, entry, compiler version |
| `Module` | path, items, content hash |
| `Node` | id, kind (`op`, `proc`, `portal`, `builtin`, `fold`, `phi`), ports, params, zone, span |
| `Port` | name, direction, type, shape |
| `Edge` | id, from (node, port), to (node, port) |
| `Region` | nodes/edges for branches and fold bodies |
| `Zone` | path (`0.1.2`), parent, members |
| `Meta` | source span, comments, canvas position |

IDs are stable across edits. Comments and canvas positions live in `Meta`, not
in the executable graph.

### 11.2 Lowering

| Source | IR |
|---|---|
| `let` / assignment | binding + edges |
| `if/elif/else` | `predicate` → branch regions → `merge` |
| `run … until` | `fold` with body region, predicate, gated feedback, fuel |
| `f.(a) g(b)` | edges: `f` outputs → `g` leftmost free inputs |
| `structured{…}` | `make_struct` + field projections |
| indexing | `index` with bounds check |
| `. / .. / ...` | static binding path (no node) |
| `input name = "…"` | source node: 0 inputs, `text: string` output, trigger |
| `output name` | sink node: `text: string` input, 0 outputs, captured result |
| `Portal` | boundary node; separate module |
| `@T(...)` | zone metadata; optional barrier |
| pipes/nodes | explicit edges |

### 11.3 Invariants

An IR is valid iff:

- every edge connects compatible types;
- every input has at most one incoming edge;
- the graph is acyclic outside `fold` regions;
- every module reference resolves and its hash matches;
- every zone path is well-formed and dependency-ordered (§12);
- every fold has a predicate and a finite fuel bound.

---

## 12. Time zones

### 12.1 Syntax

```
@T(0)      first phase
@T(0.1)    first child step of phase 0
@T(0.1.2)  third child of that step
```

A zone path is a non-empty sequence of non-negative integers separated by dots.

### 12.2 Semantics

- Paths are **lexicographic schedules**, not decimals.
- A parent gates its children; the parent completes after all children.
- Siblings under the same parent are **concurrent candidates** and MAY overlap.
- Siblings at different depths are ordered by their path.
- Zones are scheduling metadata; they do not imply threads (Q9, **OPEN**).
- `@concurrent` on a zone asserts deliberate parallelism and enables race
  reporting.
- An un-annotated node inherits `ASAP`: `1 + max(dependency zones)`.

### 12.3 Validation

- A dependency MUST NOT point from a later zone to an earlier one unless it
  crosses a declared `@async` boundary (`UFP4202`).
- Duplicate or malformed paths are errors.
- A zone with an unassigned node is reported as a warning.

---

## 13. Builtins and GGUF

### 13.1 Registry

Builtins are a closed, versioned registry. Each declares typed pins, an effect
class (`pure`, `alloc`, `io`, `model`), and a determinism guarantee.

| Builtin | Effect | Result |
|---|---|---|
| `import_gguf(path)` | io/model | nested `structured` (§13.2) |
| `tokenizer.encode/decode` | pure | `list<int>` / `string` |
| `matmul`, `transpose`, `reshape`, `softmax`, `layer_norm` | pure | tensors |
| `dequantize`, `quantize` | pure | tensor |
| `probe(value)` | pure | pass-through, emits an inspector |
| `assert(cond, msg)` | pure | runtime check |
| `len`, `all`, `any`, `sum`, `mean`, `count` | pure | reductions |

### 13.2 GGUF is nested structure

`import_gguf` returns a `structured`; no model type exists.

```
structured {
  string path,
  string version,
  structured {
    string architecture,
    int    n_layers,
    int    n_embd,
    int    n_heads,
    int    n_vocab
  } metadata,
  list matrices
}
```

Each `matrices[i]` is a `structured`:

```
structured {
  string name,
  list<int> shape,
  string dtype,
  matrix<r>[d0, …, d(r-1)] of <elem> data
}
```

The loader **specialises** the structure from the file's metadata: rank, extents,
and element type are substituted per tensor. A constant index (`matrices[0]`)
sees the specialised type; a non-constant index sees only `name`, `shape`, and
`dtype`, and `.data` is rejected (`UFP3402`). No dynamic type constructor is
introduced.

### 13.3 Quantization registry

Formats are ggml type names, versioned with the compiler:

```
q4_0 q4_1 q5_0 q5_1 q8_0
q2_k q3_k q4_k q5_k q6_k
iq1_s iq1_m iq2_xxs iq2_xs iq3_xxs iq4_xs
```

Unknown or version-mismatched formats are errors. A quantized tensor carries
blocks and scales; its `dtype` field reports the format.

---

## 14. Compilation

### 14.1 Pipeline

```
IR → partition (ggml tensor ops | scalar C) → C emission → clang → link → artifact
```

### 14.2 C runtime ABI

- Values are passed as `ufp_value` descriptors `{ptr, len, shape, elem, flags}`.
- Memory is arena-allocated per invocation; ownership is explicit (`owned` /
  `borrowed`); no GC.
- Strings and binary are length-prefixed, not NUL-terminated.
- Structured values are C structs with fields in declaration order.
- Integer widths map exactly; floats map to the corresponding C type.

### 14.3 ggml partition

- Nodes whose operands are all matrix/tensor values form a ggml graph.
- Boundaries materialise tensor handles.
- Unsupported ops fall back to C and are listed in the build manifest.
- `*` lowers to `ggml_mul_mat` with transposes inserted so source semantics stay
  row-major; `.*` → `ggml_mul`; `/` → `ggml_div`; `+`/`-` → `ggml_add`/`sub`.

### 14.4 Fold, portals, artifacts

- `run … until` becomes a do-until loop with a fuel counter and predicate.
- Each portal target compiles to its own translation unit with a generated
  header; the caller links against it.
- The driver is content-addressed: `hash(ir, flags, compiler)`.
- Output: shared library or executable, a header, and a JSON manifest listing
  artifacts, warnings, fallbacks, and the ggml version.
- Builds MUST be reproducible given identical inputs and toolchain.

### 14.5 Sandbox

- No raw C, `asm`, or filesystem access from `.ufp`.
- Builtins are the only effect surface; capabilities are granted per build.
- Generated C MUST compile without warnings promoted to errors by the driver.

---

## 15. Runtime semantics

### 15.1 Values

Values are immutable except `list` (mutable elements) and `inout` pins.

### 15.2 Errors

| Condition | Code |
|---|---|
| division by zero | `UFP5001` |
| out-of-bounds index | `UFP5002` |
| fuel exhausted | `UFP5003` |
| failed `assert` | `UFP5004` |
| quant block violation at runtime | `UFP5005` |
| non-finite in a context requiring finite | `UFP5006` |

### 15.3 Determinism

Given identical inputs, seed, compiler version, and flags, interpreter and native
backends MUST produce identical results within the documented tolerances (§18).

### 15.4 Fuel

Fuel is per fold invocation, not per program. Nested folds multiply. `@fuel`
applies to the nearest fold; a plan-level default is **OPEN** (Q10).

### 15.5 Triggering a plan with text

A run of a plan with terminals proceeds as follows:

1. Snapshot every `input` terminal's current text (the box contents).
2. Fire all inputs: each emits one `string` on its `text` pin.
3. Execute the reachable dataflow graph.
4. Capture the value delivered to every `output` terminal's `text` pin.
5. Return the named outputs plus the execution trace.

Inputs are the only trigger in this version. There is no implicit stdin fallback:
a plan without inputs is invoked through `main` or a portal. An output with no
value is a runtime error (`UFP3404`), not an empty string.

The reference CLI exposes this as `ufp run <file.ufp> [--set name=text]`; the
editor's Run button uses the same path. A trigger fires the whole terminal set;
future `@trigger(...)` annotations may narrow it.

### 15.6 Interactive run sessions and observation

Clicking **Run** creates (or focuses) a run session for the plan (D-013). The
session presents one editable text terminal for every declared `input` and one
read-only terminal for every declared `output`. The input boxes begin with their
source literals; the user may change their values for that run. Pressing **Run
with these values** snapshots them, starts execution, and updates output boxes
when their sink nodes receive a value. Saving an edited input literal remains an
explicit source edit; a transient run value does not silently rewrite the file.

The graph canvas MAY observe execution with these rules:

- the runtime emits a small event stream: `run_started`, `node_started`,
  `node_finished`, `output_updated`, `run_finished`, and `run_failed`;
- the editor highlights the currently executing node/block from this stream;
  fold bodies, portals, and zones may be highlighted as one block rather than
  every internal operation;
- **normal mode is the default:** event collection is bounded and coarse-grained,
  no tensor values are copied for display, and rendering never blocks execution;
- an optional **slow mode** inserts a user-selected delay *between emitted
  events* (including 0 ms). It changes presentation timing only — scheduling,
  values, determinism, fuel and compiled output are unchanged;
- a future step mode MAY pause at the same event boundaries. It MUST be opt-in
  and MUST NOT enable per-op tracing by default;
- if an execution is too fast for the browser to paint every event, intermediate
  highlights may be coalesced and only the latest active block is shown. The
  result and diagnostic stream remain authoritative.

Implementations MUST cap the retained event buffer and report a dropped/coalesced
count instead of consuming unbounded memory. Fine-grained tensor inspection is a
separate `probe` feature, not part of run highlighting.

---

## 16. Diagnostics

### 16.1 Format

```
<file>:<line>:<col>: <severity> <code>: <message>
  hint: <optional hint>
```

Machine-readable form (editor/compiler):

```json
{"code":"UFP3101","severity":"error","message":"…","span":{"file":"x.ufp","start":12,"end":18,"line":1,"col":13},"hint":"…"}
```

### 16.2 Ranges

| Range | Area |
|---|---|
| `UFP1xxx` | lexical |
| `UFP2xxx` | syntax |
| `UFP3xxx` | types, scopes, constraints |
| `UFP4xxx` | IR, zones, portals |
| `UFP5xxx` | runtime, compilation |
| `UFP6xxx` | builtins, GGUF, quantization |

Codes are stable once released. Appendix C lists the initial catalogue.

---

## 17. Editor contract

- The editor is a view over the IR; it MUST NOT maintain a second semantic model.
- A link is accepted only if the checker accepts it; refusals show the code and
  a short reason.
- `Portal` double-click opens the target in a nested tab with a breadcrumb.
- `input`/`output` double-click opens a text box: editable source for inputs,
  read-only result for outputs. Run creates/focuses an interactive terminal
  session, then fires all inputs with its snapshotted text.
- The Run panel highlights the current coarse execution block; normal mode is
  low-overhead, while an optional speed control delays only between events.
- `.txt` files open in a notes pane, never in the graph.
- `@T` zones render as nested bands with per-zone status.
- Autosave, undo/redo, copy/paste, and multi-select operate on IR ids.
- Export produces source + project JSON; import round-trips structure losslessly.

---

## 18. Conformance and testing

- **Syntax**: corpus of `.ufp` files parse without diagnostics; every error test
  yields the expected code and span.
- **Static**: accepted programs type-check; rejected programs name the offending
  construct.
- **Runtime**: interpreter results match reference vectors; native results match
  the interpreter within tolerance:
  - `float<32>`: `1e-5` relative;
  - `float<16>`/`bfloat<16>`: `1e-3` relative;
  - quantized matmul: format-specific, documented per format.
- **IR**: lowering snapshots one per rule.
- **Zones**: dependency/zone violations are detected.
- **Reproducibility**: artifact hashes match across runs.

---

## 19. Versioning

- The language version follows `MAJOR.MINOR.PATCH`.
- Adding a builtin, quant format, or diagnostic code is a MINOR change.
- Changing syntax or semantics of an accepted program is a MAJOR change.
- The compiler records its version in every artifact manifest and portal cache
  key. Source MAY declare `@version("1.0")`; a mismatch is a warning.

---

## 20. Open questions

| ID | Question | Current provisional behaviour |
|---|---|---|
| Q3 | `sheet` row/column mutation? | typed columns, read-only rows |
| Q4 | float comparison / integer atomics | numeric compare; NaN unordered |
| Q5 | string ordering | equality only |
| Q6 | `condition` capture | by value, explicit free-variable check |
| Q8 | portal export name | `main` default, `as` selects |
| Q9 | T zones imply threads? | scheduling only; strict mode adds barriers |
| Q10 | fuel scope | per fold, default 10 000 |
| Q11 | `.txt` as `@doc`? | inert file |
| Q12 | project root | `main` in the opened file |
| Q13 | `import` vs `portal` | builtins vs user plans |
| Q15 | ggml required? | required for tensor partitions; scalar plans build without |

---

## Appendix A — ggml mapping

| UFP | ggml |
|---|---|
| `a * b` | `ggml_mul_mat` (+ transpose insertion) |
| `a .* b` | `ggml_mul` |
| `a / b` | `ggml_div` |
| `a + b` / `a - b` | `ggml_add` / `ggml_sub` |
| `-a` | `ggml_neg` |
| `transpose(a)` | `ggml_transpose` |
| `reshape(a, shape)` | `ggml_reshape_*` |
| `softmax(a)` | `ggml_soft_max` |
| `layer_norm(a, w, b)` | composed (`ggml_norm` + mul/add) |
| `sum(a)` | `ggml_sum` (or `sum_rows`) |
| quantized `*` | `ggml_mul_mat` with type-specific kernels |

## Appendix B — GGUF layout (normative shape, not byte layout)

```
GGUF = structured {
  path: string, version: string,
  metadata: structured {
    architecture: string, n_layers: int, n_embd: int,
    n_heads: int, n_vocab: int, …
  },
  matrices: list of structured {
    name: string, shape: list<int>, dtype: string,
    data: matrix<r>[d0,…,d(r-1)] of <elem>
  }
}
```

Byte-level GGUF parsing is a builtin concern, not a language concern.

## Appendix C — initial diagnostic catalogue

| Code | Meaning |
|---|---|
| UFP1001 | unterminated string |
| UFP1002 | unterminated block comment |
| UFP1003 | invalid character |
| UFP1004 | malformed numeric literal |
| UFP1005 | invalid string escape |
| UFP2001 | expected token |
| UFP2002 | unexpected token |
| UFP2003 | malformed declaration |
| UFP2004 | malformed expression |
| UFP3001 | unknown name |
| UFP3002 | duplicate binding |
| UFP3003 | type mismatch |
| UFP3004 | arity/shape mismatch |
| UFP3005 | unresolved `auto` |
| UFP3101 | invalid field access |
| UFP3102 | invalid index arity |
| UFP3103 | non-orderable comparison |
| UFP3201 | unsolvable constraint |
| UFP3202 | rank/extent mismatch |
| UFP3203 | invalid quant format |
| UFP3204 | quant block violation |
| UFP3301 | assignment to input |
| UFP3302 | write through `..` |
| UFP3303 | non-text input/output terminal |
| UFP3401 | portal not found |
| UFP3402 | dynamic `.data` access |
| UFP3403 | duplicate input/output terminal |
| UFP3404 | output not wired |
| UFP3405 | pipe endpoint is not the terminal `text` pin |
| UFP4001 | cycle in dataflow |
| UFP4002 | duplicate/malformed zone |
| UFP4202 | zone inversion |
| UFP5001 | division by zero |
| UFP5002 | index out of bounds |
| UFP5003 | fuel exhausted |
| UFP5004 | assertion failed |
| UFP5005 | runtime quant violation |
| UFP5006 | non-finite value |
| UFP6001 | unknown builtin |
| UFP6002 | capability denied |

## Appendix D — changelog

| Version | Change |
|---|---|
| 1.0.0-draft | Initial normative draft; D-001…D-012 folded in |
