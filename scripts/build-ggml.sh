#!/usr/bin/env sh
# Build the pinned ggml submodule outside the repository tree by default.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
build_dir=${1:-"$root/.cache/ggml-build"}

if [ ! -f "$root/third_party/ggml/CMakeLists.txt" ]; then
  echo "ggml submodule is absent; run: git submodule update --init --recursive" >&2
  exit 1
fi

cmake -S "$root/third_party/ggml" -B "$build_dir" \
  -DGGML_BUILD_TESTS=OFF \
  -DGGML_BUILD_EXAMPLES=OFF \
  -DGGML_BUILD_SERVER=OFF
cmake --build "$build_dir" --config Release -j 2
