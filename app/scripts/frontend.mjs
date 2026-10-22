import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const frontend = path.join(app, 'frontend');
const mode = process.argv[2] ?? 'build';
if (mode === 'build') {
  const manifest = JSON.parse(readFileSync(path.join(app, 'migration/cubism-core.json'), 'utf8'));
  const core = readFileSync(path.join(frontend, 'public/live2d/live2dcubismcore.min.js'));
  if (core.length !== manifest.size_bytes || createHash('sha256').update(core).digest('hex') !== manifest.sha256) {
    throw new Error('Cubism Core missing or corrupt. Run python3 scripts/prepare-core.py before building.');
  }
  mkdirSync(path.join(app, 'resources/models/live2d'), {recursive: true});
  writeFileSync(path.join(app, 'resources/models/live2d/live2dcubismcore.min.js'), core);
  const check = spawnSync(process.execPath, ['node_modules/typescript/bin/tsc', '--noEmit'], {cwd: frontend, stdio: 'inherit'});
  if (check.status !== 0) process.exit(check.status ?? 1);
}
const result = spawnSync(process.execPath, ['node_modules/vite/bin/vite.js', ...(mode === 'build' ? ['build'] : [])], {cwd: frontend, stdio: 'inherit'});
process.exit(result.status ?? 1);
