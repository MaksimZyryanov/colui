# Increment 1 Final Fix Report

## Scope

Consolidated final-review fix wave. Scope remains Increment 1 domain, app, and
registry behavior. No runtime adapters, frontend, or `.DS_Store` changes.

## Important Findings

- Added runtime-free `crates/colui-domain/src/definition.rs` with exported
  projection boundary types: `ProjectDefinition`, `DefinitionRevision`,
  `DefinitionState`, `ServiceDefinition`, `Issue`, `ContainerId`,
  `ContainerInstance`, `ContainerState`, `PortBinding`, `RuntimePresence`,
  `RuntimeActivity`, and `Timestamp`.
- Registry decode and mutation validation now reject duplicate persisted
  `ProfileId` values as `AppErrorCode::RegistryCorrupt`.
- App CRUD generated-ID collisions now return `RegistryWriteFailed`; discovery
  remains the only planned owner of `ProfileAlreadyRegistered`.

## Minor Findings

- `Revision::next` now returns a typed `RegistryWriteFailed` on `u64` overflow.
- App create, update, and remove registry revision increments use checked
  arithmetic and return typed errors on overflow.
- Added duplicate-path mutation coverage and duplicate-ID load/mutation
  coverage.
- Added exhaustive serialization coverage for all 17 `AppErrorCode` variants.

## TDD Evidence

- Added regression tests before production changes.
- Initial revision-overflow test failed because `Revision::next` was infallible.
- Implemented fallible revision advancement and updated dependent callers.
- Re-ran workspace tests after implementation; all passed.

## Verification

- `cargo fmt --all -- --check`: PASS
- `cargo test --workspace`: PASS, 57 tests passed, 0 failed; doc tests passed
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`
- `git diff --check`: PASS

## Concerns

- Existing untracked `target/` directory was left untouched and is excluded
  from commit.
- Projection types are boundary placeholders only; no runtime adapters or
  conversion logic were introduced.
