#!/usr/bin/env sh
# Build and run the codegen-owned f32 matmul fixture with the pinned ggml.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
build_dir=${1:-"$root/.cache/ggml-build"}
fixture="$root/crates/ufp-codegen/fixtures/ggml_matmul.c"
binary="$build_dir/ggml-matmul-smoke"

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
