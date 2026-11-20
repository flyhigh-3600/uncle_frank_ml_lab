#!/usr/bin/env sh
# Build and run the codegen-owned f32 matmul fixture with the pinned ggml.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
build_dir=${1:-"$root/.cache/ggml-build"}
fixture="$root/crates/ufp-codegen/fixtures/ggml_matmul.c"
binary="$build_dir/ggml-matmul-smoke"
ufp_binary="$build_dir/ggml-matmul-ufp"

if [ ! -f "$root/third_party/ggml/CMakeLists.txt" ]; then
  echo "ggml submodule is absent; run: git submodule update --init --recursive" >&2
  exit 1
fi

cmake -S "$root/third_party/ggml" -B "$build_dir" \
  -DGGML_BUILD_TESTS=OFF -DGGML_BUILD_EXAMPLES=OFF -DGGML_BUILD_SERVER=OFF
cmake --build "$build_dir" --config Release -j 2

# Use the C++ driver because ggml's public API is C but the shared libraries
# are built with C++ runtime dependencies.
c++ -O2 "$fixture" -I"$root/third_party/ggml/include" \
  -L"$build_dir/src" -L"$build_dir/src/ggml-cpu" \
  -Wl,-rpath,"$build_dir/src" -Wl,-rpath,"$build_dir/src/ggml-cpu" \
  -lggml -lggml-cpu -lggml-base -lm -o "$binary"
"$binary"

UFP_GGML_SOURCE_DIR="$root/third_party/ggml" UFP_GGML_BUILD_DIR="$build_dir" \
  cargo run --manifest-path "$root/Cargo.toml" -q -p ufp-cli -- \
  compile "$root/examples/ggml_matmul.ufp" "$ufp_binary"
actual=$("$ufp_binary")
expected="out: 58 64 139 154"
if [ "$actual" != "$expected" ]; then
  echo "UFP matmul output mismatch: got '$actual', expected '$expected'" >&2
  exit 1
fi
echo "UFP ggml matmul end-to-end: ok"

compile_and_check() {
  source=$1
  name=$2
  expected=$3
  UFP_GGML_SOURCE_DIR="$root/third_party/ggml" UFP_GGML_BUILD_DIR="$build_dir" \
    cargo run --manifest-path "$root/Cargo.toml" -q -p ufp-cli -- \
    compile "$root/examples/$source" "$build_dir/$name"
  actual=$("$build_dir/$name")
  if [ "$actual" != "$expected" ]; then
    echo "$source output mismatch: got '$actual', expected '$expected'" >&2
    exit 1
  fi
  echo "$source: ok"
}

compile_and_check ggml_matrix_ops.ufp ggml-matrix-ops "out: 1 10 23 40"
compile_and_check ggml_matmul_chain.ufp ggml-matmul-chain "out: 186 447"
