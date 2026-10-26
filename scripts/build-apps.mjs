#!/usr/bin/env node
// Compile every apps/*/app.ts through the Micro compiler. This replaces the
// bash-only glob loop so `npm run build:app` works in Windows shells too.
import { spawnSync } from 'node:child_process';
import { mkdir, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
const appsDir = path.join(root, 'apps');
const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
const entries = await readdir(appsDir, { withFileTypes: true });
const apps = entries
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

let failed = false;
for (const app of apps) {
  const source = path.join(appsDir, app, 'app.ts');
  const destination = path.join(appsDir, app, 'dist', `${app}.mbc`);
  try {
    await mkdir(path.dirname(destination), { recursive: true });
  } catch (error) {
    console.error(`[build:app] cannot create ${path.dirname(destination)}: ${error.message}`);
    failed = true;
    break;
  }
  const result = spawnSync(
    cargo,
    [
      'run',
      '-p',
      'micro-compiler',
      '--bin',
      'microc',
      '--',
      path.relative(root, source),
      path.relative(root, destination),
    ],
    { cwd: root, stdio: 'inherit' },
  );
  if (result.error) {
    console.error(`[build:app] cannot run ${cargo}: ${result.error.message}`);
    failed = true;
    break;
  }
  if (result.status !== 0) {
    failed = true;
    break;
  }
}
if (failed) process.exit(1);
