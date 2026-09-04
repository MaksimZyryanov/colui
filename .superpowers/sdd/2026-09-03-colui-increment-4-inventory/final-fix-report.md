# Final Fix Evidence

Date: 2026-09-04

- Shared Compose gate: one adapter-owned `ComposeExecutionGate` is created in Tauri setup and injected into `RuntimeGateway` and `DefinitionCache`. Per-profile operation locks remain acquired before waiting for gate; permit wraps process invocation only.
- Poll semantics: `get_inventory` uses automatic refresh/backoff and React polling calls it. Existing `refresh_inventory` remains explicit and bypasses backoff per section 5.1.
- Status contract: absent/unavailable runtime projections omit inventory observation timestamp; DTO round-trip stays decodable.
- Focused regressions: 4 passed, covering cross-profile lifecycle/config serialization and release, poll suppression/manual bypass, absent app mapping, and DTO decode.
- Full Rust workspace: 151 passed, 0 failed. DTO suite: 19 passed, 0 failed.
- Frontend: lint passed; typecheck passed; 85 tests passed; contract suite 33 passed; build passed.
- Gates: formatting, dependency boundaries, boundary self-tests, Increment 4 verification, Docker-test compilation, and `git diff --check` passed.
- Real Docker smoke: skipped by expected gate (`COLUI_REAL_DOCKER_SMOKE=0`); no daemon-dependent claim.
- Expected Vitest stderr remains from tests intentionally exercising React error boundaries.
