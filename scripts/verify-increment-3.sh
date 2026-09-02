#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
temporary_dir=$(mktemp -d)
cleanup() {
  rm -rf "$temporary_dir"
}
trap cleanup EXIT

mkdir -p "$temporary_dir/schemas"
cargo run --quiet -p colui-tauri --bin schema-generator -- "$temporary_dir/schemas"
diff -ruN "$root_dir/schemas" "$temporary_dir/schemas"
