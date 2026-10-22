import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {existsSync} from 'node:fs';
import {spawnSync} from 'node:child_process';

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const python = path.join(app, 'backend/.venv', process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python');
if (!existsSync(python)) {
  console.error('Create the locked backend environment first: cd backend && uv sync --frozen');
  process.exit(1);
}
const [script, ...args] = process.argv.slice(2);
if (!script) throw new Error('A packaging script is required');
const result = spawnSync(python, [path.join(app, 'scripts', script), ...args], {cwd: app, stdio: 'inherit'});
if (result.error) console.error(result.error.message);
process.exit(result.status ?? 1);
