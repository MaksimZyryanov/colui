import ts from 'typescript';
import { readFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = process.argv[2];
const sourceRoot = join(root, 'src');
const files = ts.sys.readDirectory(sourceRoot, ['.ts', '.tsx'], undefined, undefined);
const failures = [];
const lifecycleNames = new Set(['applyProject', 'stopProject', 'tearDownProject', 'restartProject', 'apply_project', 'stop_project', 'tear_down_project', 'restart_project']);
const forbiddenFields = new Set(['compose', 'workingDirectory', 'expectedRevision', 'revision']);

for (const file of files) {
  const rel = relative(root, file).split('\\').join('/');
  const source = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true, file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  function visit(node) {
    if (ts.isImportDeclaration(node) && typeof node.moduleSpecifier.text === 'string' && node.moduleSpecifier.text.includes('@tauri-apps/api') && rel !== 'src/ipc/dispatch.ts') failures.push(`${rel}: direct Tauri import`);
    if (ts.isCallExpression(node)) {
      const name = ts.isIdentifier(node.expression) ? node.expression.text : undefined;
      if (name && lifecycleNames.has(name)) {
        for (const argument of node.arguments) {
          if (ts.isObjectLiteralExpression(argument)) {
            for (const property of argument.properties) {
              if (ts.isSpreadAssignment(property) || (ts.isPropertyAssignment(property) && forbiddenFields.has(property.name.getText(source)))) failures.push(`${rel}: lifecycle payload field`);
            }
          }
        }
      }
    }
    if (ts.isJsxAttribute(node) && node.name.text === 'key' && node.initializer) {
      const text = node.initializer.getText(source);
      if (/displayName|composeProjectName/.test(text)) failures.push(`${rel}: unstable React key`);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
if (failures.length) {
  for (const failure of failures) console.error(failure);
  process.exit(1);
}
