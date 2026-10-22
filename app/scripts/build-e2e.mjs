import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const target = path.join(app, 'src-tauri/target-e2e');
const format = process.platform === 'darwin' ? 'app' : process.platform === 'win32' ? 'nsis' : 'deb';
const build = spawnSync(process.execPath, [path.join(app, 'node_modules/@tauri-apps/cli/tauri.js'),
  'build', '--features', 'native-e2e', '--config', 'src-tauri/tauri.e2e.conf.json', '--bundles', format],
  {cwd: app, stdio: 'inherit', env: {...process.env, CARGO_TARGET_DIR: target}});
if (build.status !== 0) process.exit(build.status ?? 1);
if (process.platform === 'darwin') {
  const sign = spawnSync('python3', ['scripts/package-macos.py', '--app-bundle',
    path.join(target, 'release/bundle/macos/momo.app'), '--sign-only'], {cwd: app, stdio: 'inherit'});
  if (sign.status !== 0) process.exit(sign.status ?? 1);
}
console.log(`Native E2E build: ${target}`);
