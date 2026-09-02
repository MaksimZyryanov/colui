#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
port=${COLUI_SMOKE_PORT:-4173}
log_file=$(mktemp)
cleanup() { kill "${vite_pid:-}" 2>/dev/null || true; rm -f "$log_file"; }
trap cleanup EXIT

(cd "$root_dir" && VITE_MOCK_IPC=true npx vite --host 127.0.0.1 --port "$port" >"$log_file" 2>&1) &
vite_pid=$!
for _ in {1..50}; do
  if curl --silent --fail "http://127.0.0.1:$port/" >/dev/null; then break; fi
  sleep 0.1
done
curl --silent --fail "http://127.0.0.1:$port/src/main.tsx" >/dev/null
(cd "$root_dir" && npx vitest run src/features/projects/__tests__/browser-smoke.test.tsx)
printf 'Browser mock smoke OK\n'
