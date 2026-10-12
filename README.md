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

`pnpm test` runs application tests; saved design previews in `.agents/references/` are excluded. Run `cargo test -p colui-adapters --features test-support --test runtime` for the runtime and Compose tests not included in `cargo test --workspace`.

## Documentation

Start with [`docs/README.md`](docs/README.md) for project history, current architecture, and detailed design records.

## License

CoLUI is available under the [MIT License](LICENSE). Third-party notices are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
