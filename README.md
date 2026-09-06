# CoLUI

CoLUI is a macOS desktop application for managing local Docker Compose projects and standalone containers. It combines a React interface, a Tauri shell, and a Rust backend.

## Requirements

- macOS
- Docker Desktop or Colima with Docker Compose
- Rust stable
- Node.js and pnpm 9

## Development

```bash
pnpm install
pnpm test
cargo test --workspace
pnpm build
cargo run -p colui-tauri --bin colui-tauri
```

Use `pnpm typecheck` for TypeScript checks and `bash scripts/check-boundaries.sh` for architecture boundary checks.

## Documentation

Start with [`docs/README.md`](docs/README.md) for project history, current architecture, and detailed design records.
