#!/usr/bin/env node
/**
 * 把品牌书法字（`marks/source/name-*.webp`、`marks/source/slogan-*.webp`）**矢量化成路径**，
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
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
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

/** 预览页（自包含）：源位图 ↔ 矢量、外扩边宽度滑块、内芯/白边取色 */
function renderPreview(entries) {
  const cards = entries
    .map((entry) => {
      const bitmap = readFileSync(join(ROOT, 'marks/source', entry.source)).toString('base64');
      return `
  <section class="card">
    <h2>${entry.id} <small>${entry.source}</small></h2>
    <div class="row">
      <figure><img class="src" alt="源位图" src="data:image/webp;base64,${bitmap}"><figcaption>源位图（放大 3× 前的素材）</figcaption></figure>
      <figure class="vec">
        <svg viewBox="${entry.viewBox}" style="--outline: 0">
          <path class="mark" d="${entry.path}" fill-rule="evenodd" vector-effect="non-scaling-stroke" stroke-linejoin="round"/>
        </svg>
        <figcaption>矢量（外扩边 = stroke-width ÷ 2）</figcaption>
      </figure>
    </div>
    <label>外扩白边 <span class="num" data-num="0">0</span> px
      <input type="range" min="0" max="160" value="0" step="2" data-outline>
    </label>
  </section>`;
    })
    .join('\n');

  return `<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>raybend 品牌字矢量 · 预览</title>
<style>
  :root { color-scheme: light }
  body { margin: 0; padding: 24px; font: 14px/1.5 system-ui, "Noto Sans SC", sans-serif; background: #f6f8f4; color: #202226 }
  h1 { font-size: 18px; margin: 0 0 4px }
  p.lead { margin: 0 0 20px; color: #4a4f57 }
  .card { background: #fff; border-radius: 16px; padding: 18px; margin-bottom: 20px }
  h2 { font-size: 14px; margin: 0 0 12px; font-weight: 600 }
  h2 small { color: #71767e; font-weight: 400 }
  .row { display: grid; grid-template-columns: 1fr 1fr; gap: 16px }
  figure { margin: 0 }
  figcaption { margin-top: 6px; font-size: 12px; color: #71767e }
  img.src { width: 100%; background: #f0b033; border-radius: 8px }
  svg { width: 100%; background: #f0b033; border-radius: 8px; display: block }
  .mark { fill: var(--fill, #52c6ab); stroke: var(--stroke, #fff); stroke-width: calc(var(--outline, 0) * 1px); paint-order: stroke }
  label { display: block; margin-top: 14px; font-size: 13px; color: #4a4f57 }
  input[type=range] { width: 100%; margin-top: 6px }
  .num { font-variant-numeric: tabular-nums; color: #202226; font-weight: 600 }
</style></head>
<body>
  <h1>raybend 品牌字 · 矢量成果预览</h1>
  <p class="lead">左边是源位图，右边是 potrace 描摹出的矢量（<code>fill-rule="evenodd"</code>，否则封闭部件会被填实）。拖动滑块调外扩白边：描边是居中画的，<strong>可见的白边 = stroke-width ÷ 2</strong>。这个文件是自包含的，拷到哪儿都能打开。</p>
${cards}
<script>
  for (const card of document.querySelectorAll('.card')) {
    const svg = card.querySelector('svg');
    const slider = card.querySelector('[data-outline]');
    const num = card.querySelector('.num');
    const sync = () => { svg.style.setProperty('--outline', slider.value); num.textContent = String(Number(slider.value) / 2); };
    slider.addEventListener('input', sync);
    sync();
  }
</script>
</body></html>\n`;
}

/** 源图（白墨 + 透明底）→ 放大 → 磨掉飞白 → 合成到黑底 → 反相 → 二值化 = 黑字白底的位图 */
function prepareBitmap(file, out) {
  execFileSync('convert', [
    join(ROOT, 'marks/source', file),
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
    // 二值化阈值抬到 60%（不是默认的 50%）：
    // 前面的 `-blur` 会把细笔画「泡胀」，阈值抬高一点正好抵消 —— 实测 slogan-en 的多余墨迹
    // 从 +10.3% 降到 +9.6%，遗漏仍为 0.5%，路径体积不变。
    '60%',
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
  const width = svg.match(/width="(\d+)"/)?.[1] ?? viewBox?.split(' ')[2];
  const height = svg.match(/height="(\d+)"/)?.[1] ?? viewBox?.split(' ')[3];
  const path = svg.match(/<path[^>]*\sd="([^"]+)"/)?.[1];
  if (!viewBox || !path) throw new Error('potrace 的输出里没有 viewBox / path');
  // potrace 输出的是 **evenodd**：孔洞是绕向相反的子路径，渲染时必须带这个 fill-rule，
  // 否则「光/伴/RayBend」的封闭部件会被填实（踩过这个坑）。
  return { viewBox, width, height, path };
}

const entries = [];
for (const mark of MARKS) {
  const bitmap = join(tempDir, `${mark.id}.png`);
  prepareBitmap(mark.file, bitmap);
  const svg = await traceSvg(bitmap);
  const { viewBox, width, height, path } = extractPath(svg);
  entries.push({ id: mark.id, source: mark.file, viewBox, width, height, path });
  process.stdout.write(
    `✓ ${mark.id}: viewBox ${viewBox} · 路径 ${Math.round(path.length / 1024)} KB\n`,
  );
}

// ① SVG 留档：`marks/*.svg` —— 原始矢量成果，也是给人拿去 Inkscape/AI 继续处理的版本
const marksDir = join(ROOT, 'marks');
mkdirSync(marksDir, { recursive: true });
for (const entry of entries) {
  writeFileSync(
    join(marksDir, `${entry.id}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" width="${entry.width}" height="${entry.height}" viewBox="${entry.viewBox}">\n` +
      `  <path d="${entry.path}" fill="#52c6ab" stroke="#ffffff" stroke-width="0" stroke-linejoin="round" paint-order="stroke" vector-effect="non-scaling-stroke" fill-rule="evenodd"/>\n` +
      `</svg>\n`,
  );
}
process.stdout.write(`✓ 落盘 ${entries.length} 个 SVG → marks/*.svg\n`);

// ② 预览页：自包含（路径与源位图都内联），带描边宽度滑块 —— 调白边粗细用这个最快
writeFileSync(join(marksDir, 'preview.html'), renderPreview(entries));
process.stdout.write('✓ 生成 marks/preview.html（自包含，带外扩边滑块）\n');

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
