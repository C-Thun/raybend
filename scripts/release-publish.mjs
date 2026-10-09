#!/usr/bin/env node
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { publishRelease } from './lib/release-publish.mjs';
try {
  const [directory, ...flags] = process.argv.slice(2);
  const allowed = ['--execute', '--dry-run', '--no-crates'];
  if (!directory || new Set(flags).size !== flags.length || flags.some(f => !allowed.includes(f))) throw new Error('用法：pnpm release:publish <release-out/v版本> [--execute|--dry-run] [--no-crates]；默认只读预览，实际发布由崔总执行 --execute；--no-crates 跳过 crates.io 同步');
  publishRelease({ root: resolve(dirname(fileURLToPath(import.meta.url)), '..'), directory: resolve(directory), execute: flags.includes('--execute'), noCrates: flags.includes('--no-crates') });
} catch (error) {
  console.error(`✗ ${error.message}\n已完成的提交/tag/上传保留。排障后重跑同一发布指令；不要删除 tag 或覆盖资产。`);
  process.exitCode = 1;
}
