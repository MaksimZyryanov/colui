#!/usr/bin/env bash
set -euo pipefail

root_dir=${COLUI_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
cd "$root_dir"

bash scripts/verify-increment-3.sh
cargo fmt --all -- --check
cargo test --workspace
cargo test -p colui-tauri --test dto_contracts
cargo check -p colui-adapters --features docker-tests --tests
npx pnpm@9.15.5 lint
npx pnpm@9.15.5 typecheck
npx pnpm@9.15.5 test
npx pnpm@9.15.5 test:contracts
npx pnpm@9.15.5 build
bash scripts/check-boundaries.sh
bash scripts/check-boundaries.sh --self-test
git diff --check

if [[ ${COLUI_REAL_DOCKER_SMOKE:-0} == 1 ]]; then
  cargo test -p colui-adapters --features docker-tests --test docker
else
  printf 'Real-Docker smoke skipped; set COLUI_REAL_DOCKER_SMOKE=1 to run it.\n'
fi
