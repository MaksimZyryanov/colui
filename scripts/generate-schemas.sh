#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
cargo run --quiet -p colui-tauri --bin schema-generator -- "$root_dir/schemas"
