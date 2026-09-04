import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const checker = join(process.argv[2], 'scripts/check-boundaries.sh');

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'colui-boundaries-'));
  for (const path of ['crates/colui-domain/src', 'crates/colui-app/src', 'crates/colui-adapters/src', 'src-tauri/src/commands', 'src/ipc']) mkdirSync(join(root, path), { recursive: true });
  writeFileSync(join(root, 'crates/colui-domain/Cargo.toml'), '[dependencies]\nsafe = "1"\n');
  writeFileSync(join(root, 'crates/colui-app/Cargo.toml'), '[dependencies]\nsafe = "1"\n');
  writeFileSync(join(root, 'crates/colui-adapters/Cargo.toml'), '[dependencies]\nsafe = "1"\n');
  writeFileSync(join(root, 'crates/colui-adapters/src/inventory.rs'), 'pub(crate) struct InventoryCoordinator;\nstruct State { generation: u64 }\nfn normalize(generation: u64) {}\n');
  writeFileSync(join(root, 'crates/colui-adapters/src/definitions.rs'), 'pub struct DefinitionCache;\n');
  writeFileSync(join(root, 'crates/colui-adapters/src/operations.rs'), 'use std::collections::HashMap as Map;\ntype Leases = Map<ProfileId, ProfileState>;\npub struct OperationLockManager { profiles: Mutex<Leases> }\n');
  writeFileSync(join(root, 'src-tauri/src/commands/inventory.rs'), 'pub(crate) async fn refresh_inventory() {}\n');
  writeFileSync(join(root, 'src/ipc/commands.ts'), "export function refreshInventory() {}\napplyProject({ profileId: 'id' });\n");
  writeFileSync(join(root, 'src/app.tsx'), 'const stable = <div key={profile.id} />;\n');
  return root;
}

function run(root) {
  return spawnSync('bash', [checker], { env: { ...process.env, COLUI_ROOT: root }, encoding: 'utf8' });
}

function expectPass(label, mutate = () => {}) {
  const root = fixture();
  try {
    mutate(root);
    const result = run(root);
    if (result.status !== 0) throw new Error(`${label} should pass:\n${result.stdout}${result.stderr}`);
  } finally { rmSync(root, { recursive: true, force: true }); }
}

function expectFail(label, mutate) {
  const root = fixture();
  try {
    mutate(root);
    const result = run(root);
    if (result.status === 0) throw new Error(`${label} should fail`);
  } finally { rmSync(root, { recursive: true, force: true }); }
}

const append = (root, path, text) => writeFileSync(join(root, path), text, { flag: 'a' });
expectPass('formatted visibility and aliases');
expectPass('stable ID alias', root => append(root, 'src/app.tsx', 'const stableId = profile.id; const row = <div key={stableId} />;\n'));
expectPass('valid lifecycle variable', root => append(root, 'src/ipc/commands.ts', "const request = { profileId: 'id' }; applyProject(request);\n"));
expectFail('inline lifecycle path', root => append(root, 'src/ipc/commands.ts', "applyProject({ profileId: 'id', workingDirectory: '/tmp' });\n"));
expectFail('variable lifecycle path', root => append(root, 'src/ipc/commands.ts', "const payload = { profileId: 'id', composeFiles: ['x'] }; applyProject(payload);\n"));
for (const expression of ['container.name', 'profile.name', 'name']) expectFail(`name key ${expression}`, root => append(root, 'src/app.tsx', `const bad = <div key={${expression}} />;\n`));
expectFail('name-derived alias key', root => append(root, 'src/app.tsx', 'const rowName = container.name; const bad = <div key={rowName} />;\n'));
expectFail('direct Tauri import', root => append(root, 'src/ipc/commands.ts', "import '@tauri-apps/api/core';\n"));
expectFail('second coordinator', root => append(root, 'crates/colui-app/src/lib.rs', 'struct InventoryCoordinator;\n'));
expectFail('second definition cache', root => append(root, 'crates/colui-app/src/lib.rs', 'struct DefinitionCache;\n'));
expectFail('second operation lock owner', root => append(root, 'crates/colui-app/src/lib.rs', 'struct OperationLockManager;\n'));
expectFail('second backend refresh API', root => append(root, 'src-tauri/src/commands/inventory.rs', 'async fn refresh_inventory() {}\n'));
expectFail('second frontend refresh API', root => append(root, 'src/ipc/commands.ts', 'const refreshInventory = () => {};\n'));
expectFail('second generation counter', root => append(root, 'crates/colui-adapters/src/inventory.rs', 'struct Other { generation: u64 }\n'));
expectFail('second lock map', root => append(root, 'crates/colui-adapters/src/operations.rs', 'struct Other { profiles: Mutex<HashMap<ProfileId, ProfileState>> }\n'));
expectFail('fast Compose config', root => append(root, 'crates/colui-adapters/src/inventory.rs', 'fn poll() { compose_config(); }\n'));
expectFail('hidden registry write', root => append(root, 'crates/colui-adapters/src/inventory.rs', 'fn poll() { registry.update(); }\n'));
expectFail('forbidden dependency', root => writeFileSync(join(root, 'crates/colui-app/Cargo.toml'), '[dependencies]\ntauri = "2"\n'));

console.log('Boundary structural self-tests OK');
