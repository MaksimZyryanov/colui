#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
temporary_dir=$(mktemp -d)
committed_dir=$(mktemp -d)
cleanup() {
  rm -rf "$root_dir/schemas"
  mkdir -p "$root_dir/schemas"
  cp -R "$committed_dir/." "$root_dir/schemas/"
  rm -rf "$temporary_dir" "$committed_dir"
}
trap cleanup EXIT

cp -R "$root_dir/schemas/." "$committed_dir/"
cargo run --quiet -p colui-tauri --bin schema-generator -- "$temporary_dir/schemas"
cp -R "$temporary_dir/schemas/." "$root_dir/schemas/"
git -C "$root_dir" diff --exit-code schemas/
