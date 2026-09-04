#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# Emit dependency keys, not values. This handles regular and dotted Cargo
# dependency tables while ignoring comments and package renames.
dependency_keys() {
  awk '
    function section_name(line) {
      sub(/^\[/, "", line)
      sub(/\]$/, "", line)
      return line
    }
    {
      line = $0
      sub(/[[:space:]]*#.*/, "", line)
      sub(/^[[:space:]]*/, "", line)
      sub(/[[:space:]]*$/, "", line)
      if (line ~ /^[[:space:]]*\[[^]]+\][[:space:]]*$/) {
        section = section_name(line)
        if (section == "dependencies" || section == "dev-dependencies") {
          mode = "keys"
        } else if (section ~ /^dependencies\./) {
          sub(/^dependencies\./, "", section)
          gsub(/^"|"$/, "", section)
          print section
          mode = "other"
        } else if (section ~ /^dev-dependencies\./) {
          sub(/^dev-dependencies\./, "", section)
          gsub(/^"|"$/, "", section)
          print section
          mode = "other"
        } else {
          mode = "other"
        }
        next
      }
      if (mode == "keys" && line ~ /^[[:space:]]*[^=]+=/) {
        sub(/^[[:space:]]*/, "", line)
        sub(/[[:space:]]*=.*$/, "", line)
        gsub(/^"|"$/, "", line)
        print line
      }
    }
  ' "$1"
}

check_forbidden() {
  local manifest=$1
  local forbidden=$2
  local dependency

  while IFS= read -r dependency; do
    if [[ "$dependency" =~ ^($forbidden)$ ]]; then
      printf 'Forbidden dependency boundary in %s: %s\n' \
        "$manifest" "$dependency" >&2
      return 1
    fi
  done < <(dependency_keys "$manifest")
}

check_forbidden "$root_dir/crates/colui-domain/Cargo.toml" \
  'bollard|tauri|tokio|fs|fs2|file|filesystem|path|walk|walkdir|dir|filetime|dirs|directories|process|async-process|command|command-group'
check_forbidden "$root_dir/crates/colui-app/Cargo.toml" \
  'bollard|tauri'
check_forbidden "$root_dir/crates/colui-adapters/Cargo.toml" \
  'tauri'

reject_pattern() {
  local pattern=$1
  local description=$2
  shift 2
  if rg -n "$pattern" "$@"; then
    printf 'Boundary violation: %s\n' "$description" >&2
    exit 1
  fi
}

if [[ -f "$root_dir/crates/colui-adapters/src/inventory.rs" ]]; then
  node "$script_dir/check-boundaries-rust.mjs" "$root_dir"

  reject_pattern 'ComposeRunner|compose[ _-]?config|"config"' \
    'fast inventory path may not invoke Compose config' \
    "$root_dir/crates/colui-adapters/src/inventory.rs" "$root_dir/src-tauri/src/commands/inventory.rs"
  reject_pattern 'ProfileWriter|ProfileCreator|ProfileUpdater|ProfileRemover|registry\.(create|update|remove)|mutate_registry' \
    'inventory projection may not write profile registry' \
    "$root_dir/crates/colui-adapters/src/inventory.rs" "$root_dir/src-tauri/src/commands/inventory.rs"
fi

if [[ -d "$root_dir/src" ]]; then
  node "$script_dir/check-boundaries-node.mjs" "$root_dir"
fi

if rg -n 'Result<[^,>]*,\s*String\s*>' "$root_dir/src-tauri" --glob '*.rs'; then
  printf 'Forbidden Result error String in src-tauri\n' >&2
  exit 1
fi

if [[ ${1:-} == "--self-test" ]]; then
  node "$script_dir/check-boundaries-self-test.mjs" "$root_dir"
  fixture=$(mktemp -d)
  trap 'rm -rf "$fixture"' EXIT
  mkdir -p "$fixture/crates/colui-domain" "$fixture/crates/colui-app" \
    "$fixture/crates/colui-adapters"
  printf '%s\n' '[dependencies]' \
    'renamed = { package = "tokio", version = "1" } # ignored value' \
    '[dev-dependencies]' 'safe = "value with tauri"' \
    '[dependencies."safe-value"]' 'version = "1"' \
    > "$fixture/crates/colui-domain/Cargo.toml"
  printf '%s\n' '[dependencies]' 'safe = { package = "tauri" }' \
    '[dev-dependencies.safe-tool]' 'version = "2"' \
    > "$fixture/crates/colui-app/Cargo.toml"
  printf '%s\n' '[dependencies]' 'safe = "value with tauri"' \
    '[dev-dependencies]' 'safe = "still safe"' \
    > "$fixture/crates/colui-adapters/Cargo.toml"
  if ! COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then
    printf 'Self-test failed: dependency values were treated as keys\n' >&2
    exit 1
  fi
  for alias in file dir walk; do
    printf '%s\n' '[dependencies]' "$alias = \"1\" # forbidden alias" \
      > "$fixture/crates/colui-domain/Cargo.toml"
    if COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then
      printf 'Self-test failed: standard %s key was not detected\n' "$alias" >&2
      exit 1
    fi
    printf '%s\n' "[dependencies.$alias]" 'version = "1"' \
      > "$fixture/crates/colui-domain/Cargo.toml"
    if COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then
      printf 'Self-test failed: dotted %s key was not detected\n' "$alias" >&2
      exit 1
    fi
  done
  mkdir -p "$fixture/src/ipc"
  printf '%s\n' "import { invoke as tauriInvoke } from '@tauri-apps/api/core';" > "$fixture/src/bad-import.ts"
  if COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then printf 'Self-test failed: aliased multiline import was not detected\n' >&2; exit 1; fi
  printf '%s\n' 'import {' '  invoke as tauriInvoke' "} from '@tauri-apps/api/core';" > "$fixture/src/bad-multiline.ts"
  if COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then printf 'Self-test failed: multiline import was not detected\n' >&2; exit 1; fi
  printf '%s\n' 'applyProject({ ...payload });' > "$fixture/src/bad-spread.ts"
  if COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then printf 'Self-test failed: lifecycle spread was not detected\n' >&2; exit 1; fi
  printf '%s\n' 'const node = <div key={profile.displayName} />;' > "$fixture/src/bad-key.tsx"
  if COLUI_ROOT="$fixture" "$0" >/dev/null 2>&1; then printf 'Self-test failed: unstable JSX key was not detected\n' >&2; exit 1; fi
  printf 'Boundary parser self-test OK\n'
fi

printf 'Dependency boundaries OK\n'
