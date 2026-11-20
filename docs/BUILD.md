# Building Uncle Frank's Lab

## Rust tools

```sh
cargo test --workspace
cargo run -p ufp-cli -- run examples/text_io.ufp
cargo run -p ufp-cli -- compile examples/text_io.ufp /tmp/text_io
/tmp/text_io
```

The current compiler is a deliberately narrow proof: it emits native C for a
direct `input.text -> output.text` plan. It does **not** yet compile procedures,
matrices, portals, or models.

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

The optional first argument selects the CMake build directory. This is a native
ggml/codegen proof only: the fixture is not generated from a UFP plan, and does
not imply general UFP, model, or quantized-tensor compilation support. The
existing `ufp compile` command remains the direct text-pipe emitter.
