#!/usr/bin/env bash
set -euo pipefail

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# Check only normal dependency tables; test-only dependencies do not define
# production crate boundaries.
normal_dependencies() {
  awk '
    /^\[dependencies\]$/ { in_deps = 1; next }
    /^\[/ { in_deps = 0 }
    in_deps { print }
  ' "$1"
}

check_forbidden() {
  local manifest=$1
  local forbidden=$2
  local contents

  contents=$(normal_dependencies "$manifest")
  if printf '%s\n' "$contents" | grep -Eiq "$forbidden"; then
    printf 'Forbidden dependency boundary in %s\n' "$manifest" >&2
    return 1
  fi
}

check_forbidden "$root_dir/crates/colui-domain/Cargo.toml" \
  '(^|[-_])(bollard|tauri|tokio|fs|file|filesystem|path|walk|dir|process|command)([-_]|[[:space:]]*=|$)'
check_forbidden "$root_dir/crates/colui-app/Cargo.toml" \
  '(^|[-_])(bollard|tauri)([-_]|[[:space:]]*=|$)'
check_forbidden "$root_dir/crates/colui-adapters/Cargo.toml" \
  '(^|[-_])tauri([-_]|[[:space:]]*=|$)'

printf 'Dependency boundaries OK\n'
