#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}

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

if [[ -d "$root_dir/src" ]]; then
  if rg -n --glob '*.ts' --glob '*.tsx' "@tauri-apps/api" \
    "$root_dir/src" --glob '!src/ipc/dispatch.ts' --glob '!ipc/dispatch.ts'; then
    printf 'Forbidden direct Tauri import/invoke outside src/ipc/dispatch.ts\n' >&2
    exit 1
  fi
  if rg -n --glob '*.ts' --glob '*.tsx' --glob '!**/__tests__/**' "(applyProject|stopProject|tearDownProject|restartProject|apply_project|stop_project|tear_down_project|restart_project)\([^)]*(compose|workingDirectory|expectedRevision|revision)" "$root_dir/src"; then
    printf 'Forbidden lifecycle payload source field outside profileId\n' >&2
    exit 1
  fi
  if rg -n --glob '*.tsx' --glob '!**/__tests__/**' "key=\{[^}]*\.(displayName|composeProjectName)" "$root_dir/src"; then
    printf 'Forbidden React key based on display or Compose name\n' >&2
    exit 1
  fi
fi

if rg -n 'Result<[^,>]*,\s*String\s*>' "$root_dir/src-tauri" --glob '*.rs'; then
  printf 'Forbidden Result error String in src-tauri\n' >&2
  exit 1
fi

if [[ ${1:-} == "--self-test" ]]; then
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
  printf 'Boundary parser self-test OK\n'
fi

printf 'Dependency boundaries OK\n'
