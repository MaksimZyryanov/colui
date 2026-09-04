import ts from 'typescript';
import { readFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.argv[2];
const sourceRoot = join(root, 'src');
const files = ts.sys.readDirectory(sourceRoot, ['.ts', '.tsx'], undefined, undefined);
const failures = [];
const lifecycleNames = new Set(['applyProject', 'stopProject', 'tearDownProject', 'restartProject', 'apply_project', 'stop_project', 'tear_down_project', 'restart_project']);

function declarationInitializer(identifier, source) {
  const symbol = identifier.text;
  let found;
  function search(node) {
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.name.text === symbol && node.initializer && node.pos < identifier.pos) found = node.initializer;
    ts.forEachChild(node, search);
  }
  search(source);
  return found;
}

function lifecyclePayloadIsSafe(expression, source, seen = new Set()) {
  if (ts.isParenthesizedExpression(expression)) return lifecyclePayloadIsSafe(expression.expression, source, seen);
  if (ts.isIdentifier(expression)) {
    if (seen.has(expression.text)) return false;
    seen.add(expression.text);
    const initializer = declarationInitializer(expression, source);
    return initializer ? lifecyclePayloadIsSafe(initializer, source, seen) : false;
  }
  if (!ts.isObjectLiteralExpression(expression) || expression.properties.length !== 1) return false;
  const property = expression.properties[0];
  return (ts.isShorthandPropertyAssignment(property) && property.name.text === 'profileId')
    || (ts.isPropertyAssignment(property) && property.name.getText(source).replace(/["']/g, '') === 'profileId');
}

function isNameDerived(expression, source, seen = new Set()) {
  if (ts.isJsxExpression(expression)) return expression.expression ? isNameDerived(expression.expression, source, seen) : false;
  if (ts.isParenthesizedExpression(expression)) return isNameDerived(expression.expression, source, seen);
  if (ts.isPropertyAccessExpression(expression)) return ['name', 'displayName', 'composeProjectName'].includes(expression.name.text) || isNameDerived(expression.expression, source, seen);
  if (ts.isIdentifier(expression)) {
    if (expression.text === 'name') return true;
    if (seen.has(expression.text)) return false;
    seen.add(expression.text);
    const initializer = declarationInitializer(expression, source);
    return initializer ? isNameDerived(initializer, source, seen) : false;
  }
  let derived = false;
  ts.forEachChild(expression, child => { if (isNameDerived(child, source, seen)) derived = true; });
  return derived;
}

for (const file of files) {
  const rel = relative(root, file).split('\\').join('/');
  if (rel.includes('/__tests__/') || rel.endsWith('/mock-backend.ts')) continue;
  const source = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true, file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  function visit(node) {
    if (ts.isImportDeclaration(node) && typeof node.moduleSpecifier.text === 'string' && node.moduleSpecifier.text.includes('@tauri-apps/api') && rel !== 'src/ipc/dispatch.ts') failures.push(`${rel}: direct Tauri import`);
    if (ts.isCallExpression(node)) {
      const name = ts.isIdentifier(node.expression) ? node.expression.text : undefined;
      if (name && lifecycleNames.has(name)) {
        if (node.arguments.length !== 1 || !lifecyclePayloadIsSafe(node.arguments[0], source)) failures.push(`${rel}: lifecycle payload must contain only profileId`);
      }
    }
    if (ts.isJsxAttribute(node) && node.name.text === 'key' && node.initializer) {
      if (isNameDerived(node.initializer, source)) failures.push(`${rel}: unstable React key`);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
let refreshApis = 0;
for (const file of files) {
  if (file.includes('__tests__') || file.endsWith('mock-backend.ts')) continue;
  const source = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true, file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  function count(node) {
    if ((ts.isVariableDeclaration(node) || ts.isFunctionDeclaration(node)) && node.name && ts.isIdentifier(node.name) && node.name.text === 'refreshInventory') refreshApis += 1;
    ts.forEachChild(node, count);
  }
  count(source);
}
if (refreshApis !== 1) failures.push(`expected 1 frontend refresh API, found ${refreshApis}`);
if (failures.length) {
  for (const failure of failures) console.error(failure);
  process.exit(1);
}
