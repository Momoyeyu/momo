import path from 'node:path';
import { fileURLToPath } from 'node:url';
const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const builtBinary = process.platform === 'darwin'
  ? 'src-tauri/target-e2e/release/bundle/macos/momo.app/Contents/MacOS/momo'
  : `src-tauri/target-e2e/release/momo${process.platform === 'win32' ? '.exe' : ''}`;
const binary = process.env.MOMO_E2E_APP ?? path.join(app, builtBinary);
export const config = {
  runner: 'local',
  specs: ['./*.e2e.mjs'],
  maxInstances: 1,
  capabilities: [{browserName: 'tauri', 'tauri:options': {application: binary}}],
  services: [['tauri', {appBinaryPath: binary, driverProvider: 'embedded', embeddedPort: 4445,
    captureBackendLogs: false, captureFrontendLogs: false}]],
  before: async function () {
    // Explicit window selection suppresses the service's legacy WDIO-plugin
    // focus probes; all tests keep using real W3C commands and actual IPC.
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 0);
    const [handle] = await browser.getWindowHandles();
    await browser.switchToWindow(handle);
  },
  framework: 'mocha', reporters: ['spec'], logLevel: 'warn',
  mochaOpts: {timeout: 120000}, waitforTimeout: 60000,
  connectionRetryTimeout: 120000,
};
