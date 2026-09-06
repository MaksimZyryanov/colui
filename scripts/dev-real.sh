#!/usr/bin/env bash
set -Eeuo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

VITE_MOCK_IPC=false pnpm exec vite \
  --host 127.0.0.1 \
  --port 1420 \
  --strictPort &
vite_pid=$!

cleanup() {
  kill "$vite_pid" 2>/dev/null || true
  wait "$vite_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

VITE_MOCK_IPC=false pnpm exec tauri dev \
  --config '{"build":{"devUrl":"http://127.0.0.1:1420"}}'
