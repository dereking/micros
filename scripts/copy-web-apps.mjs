#!/usr/bin/env node
// Stage the compiled MBC files for the Web Player without relying on POSIX
// `mkdir -p`/`cp`, so the same command works on Windows.
import { copyFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
const apps = ['counter', 'shell', 'settings', 'momo'];
const destination = path.join(root, 'products', 'micro-web-player', 'public', 'apps');
await mkdir(destination, { recursive: true });
for (const app of apps) {
  const source = path.join(root, 'apps', app, 'dist', `${app}.mbc`);
  await copyFile(source, path.join(destination, `${app}.mbc`));
}
