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

`ufp-codegen` will link this pinned copy when UF-603 implements typed tensor
partitions. Until then, the submodule is built and version-pinned but not linked
by the text-only native slice.
