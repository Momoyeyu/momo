console.error(`Native tauri dev is unsupported: the shell requires packaged resources and native-origin authentication.
For browser hot reload, use the isolated two-terminal commands in docs/development/migration.md (from the repository root).
For automated browser checks: node scripts/browser-e2e.mjs --smoke (from app/).
For native checks, prepare the frozen backend/models, then use pnpm build or pnpm build:e2e; see app/README.md.`);
process.exit(1);
