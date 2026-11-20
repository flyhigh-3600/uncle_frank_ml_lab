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

It declares two row-major `float<32>` matrices and a zero-argument
`procedure main()` containing `let out = a * b; return out;`. It prints
`out: 58 64 139 154`. This is the supported UFP subset: exactly two initialized
rank-2 matrices, flat numeric literal initializers, compatible dimensions, and
that one multiplication procedure shape. Other element types, dynamic
dimensions, pins, multiple operations, calls, portals, models, and general UFP
are rejected. `UFP_GGML_SOURCE_DIR` and `UFP_GGML_BUILD_DIR` override source/build
locations. The direct text-pipe `ufp compile` path remains unchanged.
