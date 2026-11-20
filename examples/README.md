# Examples — the intended shape of `.ufp`

These are **design targets**, not yet parsed by anything: the lexer/parser work is
UF-101…UF-103. They exist so the grammar stays honest. Expect churn until the
decision log settles Q1–Q15.

| File | Exercises |
|---|---|
| `hello.ufp` | procedures, pins, matrix literal, indexing |
| `predict.ufp` | relational pin constraints, `if/elif/else` |
| `structured.ufp` | `structured`, dot access, element-wise comparison |
| `portal_demo.ufp` | `Portal`, top-level nodes, pipes, `@T` zones |
| `generate.ufp` | do-until fold (`run … until`), `.`, `..`, `condition` |
| `quantized.ufp` | `float<16>`, `quant<q4_0>`, matmul vs explicit `dequantize` |
| `gguf.ufp` | nested `structured` model, `matrices[0]`, `..` scope walking (D-011) |
| `text_io.ufp` | text terminals: `input` trigger, `output` sink, raw text block (D-012) |
| `model_layers.ufp` + `layers/layer_000.ufp` | folder workspace, compact inspectable portal, named nested matrix (D-014) |

`.txt` files are inert; see `notes.txt`.
