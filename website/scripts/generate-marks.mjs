#!/usr/bin/env node
/**
 * 把品牌书法字（`public/name-*.webp`、`public/slogan-*.webp`）**矢量化成路径**，
 * 生成 `src/components/mark-paths.ts`。
 *
 * **为什么要矢量化**：hero 上要给产品名/口号套一圈「白色外扩边 + 绿色内芯」。
 * 位图做不到 —— CSS 的 `mask-image` 只能给**墨迹本身**上色，无法向外扩张出血边；
 * 用 `drop-shadow` 叠出来的只能算模糊光晕，`feMorphology` 在部分浏览器上不可靠。
 * 矢量化之后就是一条 `<path>`：`fill` 染绿、`stroke` 描白、`paint-order="stroke"`
 * 让描边压在填充下面，于是只剩**向外那一半**，即「外扩边」。
 *
 * 做法：ImageMagick 放大并二值化 → potrace 描摹 → 取出一条 path 写进生成物。
 * 只影响构建期，不引入运行时依赖（`potrace` 是 devDependency）。
 *
 *   pnpm marks:generate
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import potrace from 'potrace';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

/** 放大倍数：源图只有 700～1300 px 宽，放大后再描摹，曲线更顺（路径坐标也在这个尺度上） */
const SCALE = 3;

/** 描摹参数：
 *  - 先 `-blur 0x3` 再二值化，把毛笔飞白/扫描噪点磨掉 —— 不磨的话路径会大 5 倍（实测 97KB → 19KB）；
 *  - `optTolerance` 控制曲线简化程度，太大笔锋会变圆。
 */
const TRACE_OPTIONS = {
  threshold: 128,
  turdSize: 8,
  alphamax: 1,
  optCurve: true,
  optTolerance: 0.4,
  blackOnWhite: true,
};

const MARKS = [
  { id: 'name-zh', file: 'name-cn.webp' },
  { id: 'name-en', file: 'name-en.webp' },
  { id: 'slogan-zh', file: 'slogan-cn.webp' },
  { id: 'slogan-en', file: 'slogan-en.webp' },
];

const tempDir = mkdtempSync(join(tmpdir(), 'rb-marks-'));

/** 源图（白墨 + 透明底）→ 放大 → 磨掉飞白 → 合成到黑底 → 反相 → 二值化 = 黑字白底的位图 */
function prepareBitmap(file, out) {
  execFileSync('convert', [
    join(ROOT, 'public', file),
    '-filter',
    'Lanczos',
    '-resize',
    `${SCALE * 100}%`,
    '-blur',
    '0x3',
    '-background',
    'black',
    '-alpha',
    'remove',
    '-alpha',
    'off',
    '-negate',
    '-threshold',
    '50%',
    '-strip',
    out,
  ]);
}

function traceSvg(bitmapPath) {
  return new Promise((resolve, reject) => {
    potrace.trace(bitmapPath, TRACE_OPTIONS, (error, svg) => {
      if (error) reject(error);
      else resolve(svg);
    });
  });
}

/** 从 potrace 的 SVG 里取出 viewBox 与那一条 `<path d>` */
function extractPath(svg) {
  const viewBox = svg.match(/viewBox="([^"]+)"/)?.[1];
  const path = svg.match(/<path[^>]*\sd="([^"]+)"/)?.[1];
  if (!viewBox || !path) throw new Error('potrace 的输出里没有 viewBox / path');
  return { viewBox, path };
}

const entries = [];
for (const mark of MARKS) {
  const bitmap = join(tempDir, `${mark.id}.png`);
  prepareBitmap(mark.file, bitmap);
  const svg = await traceSvg(bitmap);
  const { viewBox, path } = extractPath(svg);
  entries.push({ id: mark.id, source: mark.file, viewBox, path });
  process.stdout.write(
    `✓ ${mark.id}: viewBox ${viewBox} · 路径 ${Math.round(path.length / 1024)} KB\n`,
  );
}

const file = join(ROOT, 'src/components/mark-paths.ts');
writeFileSync(
  file,
  `/**
 * 品牌书法字的矢量路径。
 *
 * ⚠️ **本文件由 \`scripts/generate-marks.mjs\` 生成，不要手改** ——
 * 改素材（\`public/name-*.webp\` / \`slogan-*.webp\`）后跑 \`pnpm marks:generate\`。
 *
 * 路径坐标是「源图放大 ${SCALE} 倍」后的尺度（\`viewBox\` 也在这个尺度上），
 * 渲染时用 CSS 宽度控制实际大小；\`fill\`/\`stroke\` 由 \`BrandMark\` 决定。
 */

export interface MarkPath {
  /** 源素材文件名（改素材时对照用） */
  source: string;
  viewBox: string;
  path: string;
}

export const MARK_PATHS = {
${entries
  .map(
    (entry) =>
      `  '${entry.id}': {\n    source: '${entry.source}',\n    viewBox: '${entry.viewBox}',\n    path:\n      '${entry.path}',\n  },`,
  )
  .join('\n')}
} as const satisfies Record<string, MarkPath>;

export type MarkId = keyof typeof MARK_PATHS;
`,
);
process.stdout.write(`✓ 生成 ${MARKS.length} 条路径 → src/components/mark-paths.ts\n`);
