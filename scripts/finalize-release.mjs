#!/usr/bin/env node
/** 由崔总在生成/签名安装器后执行；不上传、不创建 tag。 */
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, readdirSync, existsSync, mkdirSync } from 'node:fs';
import { resolve, join, basename, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256 } from './lib/release-files.mjs';
import { releaseMetadata } from './lib/release-artifacts.mjs';

export function finalizeRelease({ argv = process.argv.slice(2), run = execFileSync, log = console.log, selectVersion, requiredTargets = [] } = {}) {
  const [directory, ...args] = argv;
  const options = {};
  let allowUnsigned = false;
  if (!directory) throw new Error('用法：pnpm release:finalize <bundle目录> --manifest <raybend-build.json> --out <全新输出目录> [--base-url <HTTPS下载基址>] [--allow-unsigned]');
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--allow-unsigned') {
      if (allowUnsigned) throw new Error('重复参数');
      allowUnsigned = true;
      continue;
    }
    if (!['--manifest', '--out', '--base-url'].includes(arg) || !args[i + 1] || args[i + 1].startsWith('--') || options[arg]) throw new Error('非法/重复参数');
    options[arg] = args[++i];
  }
  if (!options['--manifest'] || !options['--out']) throw new Error('缺少 manifest/out');
  const out = resolve(options['--out']);
  if (existsSync(out)) throw new Error('输出目录已存在；请选全新目录，避免覆盖并行文件或混入旧包');
  const artifacts = [], contents = new Map();
  const walk = dir => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isSymbolicLink()) throw new Error('发布目录不允许符号链接');
      if (entry.isDirectory()) walk(path);
      else if (/\.(exe|msi)$/i.test(entry.name)) {
        if (selectVersion && !entry.name.startsWith(`RayBend_${selectVersion}_`)) continue;
        const bytes = readFileSync(path);
        let verified = false;
        if (!allowUnsigned) {
          const windowsPath = String(run('wslpath', ['-w', path], { encoding: 'utf8' })).trim();
          run('signtool.exe', ['verify', '/pa', '/all', windowsPath], { stdio: 'inherit' });
          verified = true;
        }
        const signature = options['--base-url'] && existsSync(path + '.sig') ? readFileSync(path + '.sig', 'utf8') : undefined;
        const name = basename(path);
        artifacts.push({ name, sha256: sha256(bytes), bytes: bytes.length, verified, signature });
        contents.set(name, bytes);
      }
    }
  };
  walk(resolve(directory));
  for (const target of requiredTargets) {
    const matches=artifacts.filter(a=>target==='nsis'?a.name.toLowerCase().endsWith('-setup.exe'):a.name.toLowerCase().endsWith('.msi'));
    if(matches.length!==1)throw new Error(`本版本 ${target} 安装器应恰有一个，实际 ${matches.length}`);
  }
  artifacts.sort((a, b) => a.name.localeCompare(b.name));
  const manifest = JSON.parse(readFileSync(resolve(options['--manifest']), 'utf8'));
  const metadata = releaseMetadata({ manifest, artifacts, baseUrl: options['--base-url'], withUpdater: !!options['--base-url'], unsigned: allowUnsigned });
  mkdirSync(dirname(out), { recursive: true });
  mkdirSync(out); // 独占创建，外部并行创建时直接失败。
  const write = (name, data) => writeFileSync(join(out, name), data, { flag: 'wx' });
  for (const file of artifacts) {
    write(file.name, contents.get(file.name));
    if (file.signature !== undefined) write(file.name + '.sig', file.signature);
  }
  write('raybend-build.json', JSON.stringify(manifest, null, 2) + '\n');
  write('SHA256SUMS', metadata.checksums);
  write('release-index.json', JSON.stringify(metadata.index, null, 2) + '\n');
  if (metadata.update) write(manifest.channel === 'beta' ? 'beta.json' : 'latest.json', JSON.stringify(metadata.update, null, 2) + '\n');
  write('RELEASE-NOTES.md', `# RayBend ${manifest.version}\n\nWindows x64 · ${manifest.channel}\n\n导入、浏览评级筛选、非破坏性编辑与多定稿、WebP/AVIF/JPG/PNG 导出、RGB16 TIFF 单向外部编辑交接。\n\n${allowUnsigned ? '此包未使用 Windows 发布者签名；Windows 可能提示未知发布者或阻止运行。' : '最终安装器已通过本机 Authenticode 验证。'} 不保证 SmartScreen 信誉，勿关闭系统保护作为通用安装步骤。\n\n${metadata.update ? '本发布包含签名更新清单；应用仅在主动检查时联网。' : '本发布未提供签名更新清单；请通过发布页手动更新。'}\n\n校验安装器：下载 SHA256SUMS 并核对 SHA-256。对应源码：https://github.com/C-Thun/raybend/tree/v${manifest.version}\n使用说明：https://github.com/C-Thun/raybend/blob/v${manifest.version}/docs/user-guide.md\n\n发布前请按 docs/release.md 完成真机验收，核对此说明的功能/限制与实际结果，并补充本版变更。\n`);
  log(`已准备 ${artifacts.length} 个安装器、签名/哈希/清单与说明到 ${out}；未上传。运行 pnpm release:publish ${out} 只读核对并预览人工上传命令。`);
  return metadata;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { finalizeRelease(); } catch (error) { console.error(`✗ ${error.message}`); process.exitCode = 1; }
}
