# Building Uncle Frank's Lab

## Rust tools

```sh
cargo test --workspace
cargo run -p ufp-cli -- run examples/text_io.ufp
cargo run -p ufp-cli -- compile examples/text_io.ufp /tmp/text_io
/tmp/text_io
```

The compiler supports direct text pipes plus the narrow ggml matrix procedure
subset described below. It does not support general procedures, portals, or
models.

## ggml

The repository pins upstream [ggml](https://github.com/ggml-org/ggml) as the
`third_party/ggml` Git submodule. Initialise it after cloning:

```sh
git submodule update --init --recursive
./scripts/build-ggml.sh
```

The script configures a CPU build with tests/examples/server disabled. Pass an
alternative build directory as its first argument. The generated build directory
is local and must not be committed.

The codegen crate also owns a standalone f32 matrix-multiplication C fixture.
It exercises `ggml_mul_mat` on the pinned CPU backend and compares all four
results against known values:

```sh
./scripts/ggml-matmul-smoke.sh
```

The optional first argument selects the CMake build directory. The script runs
both the standalone C fixture and an end-to-end UFP fixture generated through
the CLI.

After building ggml, the UFP fixture can be compiled and run directly:

```sh
cargo run -p ufp-cli -- compile examples/ggml_matmul.ufp /tmp/ufp-matmul
/tmp/ufp-matmul
```

The generated program formats matrices in row form (e.g.
`out: [[58, 64], [139, 154]]`) and exports a C ABI. For a zero-input procedure
the entry point is `int ufp_execute(float *output, size_t output_len)`. A
procedure with typed matrix inputs receives one `const float *input_NAME,
size_t input_NAME_len` pair per input, followed by the output pointer and
length. Input/output buffers are row-major `float` arrays; declared dimensions
and operators are checked during codegen, and the ABI rejects null or wrong
length buffers. Top-level literal matrices remain embedded weights. The CLI
program's `main` supplies deterministic 1..N values for input pins; external C
callers provide their own buffers. The smoke script tests both forms from C.
The original fixture declares row-major `float<32>` matrices and a zero-argument
`procedure main()`. `ggml_input_abi.ufp` exercises an input pin with an embedded
weight. `ggml_matrix_ops.ufp` additionally exercises nested and
flat initializers, a typed local, and `+`, `-`, and `.*`; `ggml_matmul_chain.ufp`
exercises three matrices and a two-step matmul graph. This remains a deliberately
small subset: top-level initialized rank-2 f32 matrices with literal dimensions,
and sequential local `let` bindings of matrix names/binary operations followed
by a matrix return. Supported operations are shape-checked `+`, `-`, `.*`, and
`*` (matrix multiplication). Other element types, dynamic dimensions, inout or
multiple output pins, control flow, calls, annotations, portals, models, and
general UFP are rejected. `UFP_GGML_SOURCE_DIR` and `UFP_GGML_BUILD_DIR`
override source/build locations. The direct text-pipe `ufp compile` path remains
unchanged.
