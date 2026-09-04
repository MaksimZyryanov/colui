import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const root = process.argv[2];
const failures = [];
const production = [
  'crates/colui-domain/src',
  'crates/colui-app/src',
  'crates/colui-adapters/src',
  'src-tauri/src',
].flatMap(path => {
  const directory = join(root, path);
  return existsSync(directory) ? rustFiles(directory) : [];
});

function rustFiles(directory) {
  const files = [];
  const stack = [directory];
  while (stack.length) {
    const current = stack.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      if (entry.isDirectory()) stack.push(join(current, entry.name));
      else if (entry.name.endsWith('.rs')) files.push(join(current, entry.name));
    }
  }
  return files;
}

function tokens(file) {
  const source = readFileSync(file, 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/\/\/[^\n]*/g, ' ')
    .replace(/r#*"[\s\S]*?"#*/g, '""')
    .replace(/"(?:\\.|[^"\\])*"/g, '""')
    .replace(/'(?:\\.|[^'\\])'/g, "''");
  return source.match(/[A-Za-z_][A-Za-z0-9_]*|::|->|[{}()<>,:;]/g) ?? [];
}

const parsed = production.map(file => ({ file, tokens: tokens(file) }));
function countSequence(sequence) {
  let count = 0;
  for (const { tokens } of parsed) for (let i = 0; i <= tokens.length - sequence.length; i += 1) {
    if (sequence.every((token, offset) => tokens[i + offset] === token)) count += 1;
  }
  return count;
}
function requireCount(sequence, expected, label) {
  const count = countSequence(sequence);
  if (count !== expected) failures.push(`expected ${expected} ${label}, found ${count}`);
}

for (const owner of ['InventoryCoordinator', 'DefinitionCache', 'OperationLockManager']) requireCount(['struct', owner], 1, `${owner} owner`);
requireCount(['fn', 'refresh_inventory'], 1, 'backend refresh API');

const inventory = parsed.find(({ file }) => file === join(root, 'crates/colui-adapters/src/inventory.rs'));
if (inventory) {
  let generationFields = 0;
  for (let i = 0; i < inventory.tokens.length - 2; i += 1) {
    if (inventory.tokens[i] === 'generation' && inventory.tokens[i + 1] === ':' && insideStruct(inventory.tokens, i)) generationFields += 1;
  }
  if (generationFields !== 1) failures.push(`expected 1 inventory generation counter, found ${generationFields}`);
}

const operations = parsed.find(({ file }) => file === join(root, 'crates/colui-adapters/src/operations.rs'));
if (operations) {
  const aliases = new Map();
  for (let i = 0; i < operations.tokens.length; i += 1) if (operations.tokens[i] === 'type') {
    const name = operations.tokens[i + 1];
    const end = operations.tokens.indexOf(';', i);
    aliases.set(name, operations.tokens.slice(i + 2, end));
  }
  let maps = 0;
  for (let i = 0; i < operations.tokens.length; i += 1) if (insideStruct(operations.tokens, i) && operations.tokens[i] === 'Mutex' && operations.tokens[i + 1] === '<') {
    const type = aliases.get(operations.tokens[i + 2]) ?? operations.tokens.slice(i + 2, i + 9);
    if (type.includes('HashMap') || type.includes('Map')) maps += 1;
  }
  if (maps !== 1) failures.push(`expected 1 operation lock map, found ${maps}`);
}

function insideStruct(values, index) {
  let depth = 0;
  for (let i = index; i >= 0; i -= 1) {
    if (values[i] === '}') depth += 1;
    if (values[i] === '{') {
      if (depth > 0) depth -= 1;
      else return values.slice(Math.max(0, i - 4), i).includes('struct');
    }
  }
  return false;
}

if (failures.length) {
  failures.forEach(failure => console.error(`Rust architecture boundary: ${failure}`));
  process.exit(1);
}
