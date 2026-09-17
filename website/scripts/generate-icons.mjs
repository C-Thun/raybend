#!/usr/bin/env node
/**
 * 从 `lucide-solid` 的图标数据生成 `src/components/icons.tsx`。
 *
 * **为什么不用 `lucide-solid` 的组件**：它的 1.x 组件是从 `solid-js` 里 import
 * `splitProps` 的，而 Solid 2 已经删掉了这个 API（连 `mergeProps` 也搬去了 `@solidjs/web`），
 * 直接用它会在浏览器里报「does not provide an export named 'splitProps'」。
 * 官方也还没有支持 Solid 2 的版本（最新 1.46.0 的 peer 仍是 `solid-js: ^1.4.7`）。
 *
 * 所以这里只**取它的图标数据**（Lucide 图标本体是 ISC 许可，允许这样用），
 * 生成一份本站自己的、Solid 2 友好的组件 —— 顺带让产物里只留下用到的十几个图标，
 * 而不是整个包。加图标：往下面的清单里加一行，再跑
 *
 *   pnpm icons:generate
 *
 * 许可与出处见 `website/ASSETS.md` 与 `THIRD-PARTY-NOTICES.md`。
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const ICONS_DIR = join(ROOT, 'node_modules/lucide-solid/dist/esm/icons');

/** 导出名 → lucide 图标名（kebab-case） */
const ICONS = {
  IconBolt: 'zap',
  IconCheckCircle: 'circle-check',
  IconChip: 'cpu',
  IconDownload: 'download',
  IconFolderInput: 'folder-input',
  IconGrid: 'layout-grid',
  IconHardDrive: 'hard-drive',
  IconImage: 'image',
  IconLock: 'lock',
  IconMonitorDown: 'monitor-down',
  IconPlayCircle: 'circle-play',
  IconShieldCheck: 'shield-check',
  IconTags: 'tags',
  IconTree: 'git-branch',
  IconUpload: 'upload',
  IconVideo: 'video',
};

/** `stroke-linecap` → `strokeLinecap`（React/Solid 的 JSX 属性写法） */
const toJsxAttribute = (name) =>
  name.includes('-') ? name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase()) : name;

/** 属性值：数字不加引号，其余用双引号包起来 */
const renderAttribute = (name, value) =>
  typeof value === 'number' ? `${toJsxAttribute(name)}={${value}}` : `${toJsxAttribute(name)}="${value}"`;

/**
 * 从 lucide 的模块里取出图标节点。
 *
 * 模块是生成出来的、格式固定的数据（`node: [['circle', { cx: '12', … }], …]`），
 * 所以这里用正则把节点抠出来，而**不用 `eval`/`new Function` 去求值源码** ——
 * 图标文件是第三方产物，不该在构建流程里执行它的代码。
 */
const NODE_PATTERN = /\[\s*'(circle|ellipse|line|path|polygon|polyline|rect)',\s*\{([^}]*)\}\s*\]/g;
const ATTRIBUTE_PATTERN = /([A-Za-z][\w-]*):\s*(?:'([^']*)'|"([^"]*)"|([\d.-]+))/g;

function readIconData(lucideName) {
  const source = readFileSync(join(ICONS_DIR, `${lucideName}.mjs`), 'utf8');
  const nodeStart = source.indexOf('node: [');
  // 图标数据的结尾一律是 `};`（节点数组的收尾行数不固定，不能按 \n  ] 找）
  const nodeEnd = source.indexOf('\n};', nodeStart);
  if (nodeStart < 0 || nodeEnd < 0) throw new Error(`解析不到 ${lucideName} 的图标数据`);

  const slice = source.slice(nodeStart, nodeEnd);
  const nodes = [];
  for (const match of slice.matchAll(NODE_PATTERN)) {
    const [, tag, rawAttributes] = match;
    const attributes = {};
    for (const attribute of rawAttributes.matchAll(ATTRIBUTE_PATTERN)) {
      const [, name, single, double, numeric] = attribute;
      // `key` 是 lucide 内部用的稳定 key，DOM 用不到
      if (name === 'key') continue;
      const raw = single ?? double ?? numeric;
      attributes[name] = single !== undefined || double !== undefined ? raw : Number(raw);
    }
    nodes.push([tag, attributes]);
  }
  if (nodes.length === 0) throw new Error(`${lucideName} 没解析出任何节点`);
  return { node: nodes };
}

function renderIcon(exportName, lucideName) {
  const data = readIconData(lucideName);
  const children = data.node
    .map(([tag, attributes]) => {
      const props = Object.entries(attributes)
        // `key` 是 lucide 内部用的稳定 key，DOM 用不到
        .filter(([name]) => name !== 'key')
        .map(([name, value]) => renderAttribute(name, value))
        .join(' ');
      return `      <${tag} ${props} />`;
    })
    .join('\n');

  return `/** \`${lucideName}\` — https://lucide.dev/icons/${lucideName} */
export function ${exportName}(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
${children}
    </IconBase>
  );
}`;
}

const header = `/**
 * 本站用到的 Lucide 图标。
 *
 * ⚠️ **本文件由 \`scripts/generate-icons.mjs\` 生成，不要手改** ——
 * 要加图标就往那个脚本的清单里加一行，再跑 \`pnpm icons:generate\`。
 *
 * 只取 Lucide 的**图标数据**（ISC 许可，见 \`THIRD-PARTY-NOTICES.md\`），组件本身是本站写的：
 * \`lucide-solid\` 1.x 依赖 Solid 1 的 \`splitProps\`，在 Solid 2 下无法使用。
 */
import type { JSX } from '@solidjs/web';

export interface IconProps {
  /** 尺寸与颜色走类名（如 \`size-5 text-brand-dark\`）；接受 Solid 2 的字符串/数组/对象写法 */
  class?: JSX.ClassValue;
  /** 线宽，默认 2（Lucide 的默认值） */
  strokeWidth?: number;
}

function IconBase(props: { children: JSX.Element } & IconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      // 注意：Solid 2 的 JSX 类型只认 kebab-case 的 SVG 属性（写成驼峰会 TS2322）——
      // 下面三行不是笔误，是这套类型定义的硬要求，因此逐行压制该提示。
      // pi-lens-ignore: hyphenated-svg-attribute
      stroke-width={props.strokeWidth ?? 2}
      // pi-lens-ignore: hyphenated-svg-attribute
      stroke-linecap="round"
      // pi-lens-ignore: hyphenated-svg-attribute
      stroke-linejoin="round"
      aria-hidden="true"
      class={props.class}
    >
      {props.children}
    </svg>
  );
}

`;

const body = Object.entries(ICONS)
  .map(([exportName, lucideName]) => renderIcon(exportName, lucideName))
  .join('\n\n');

const out = join(ROOT, 'src/components/icons.tsx');
writeFileSync(out, `${header}${body}\n`);
process.stdout.write(`✓ 生成 ${Object.keys(ICONS).length} 个图标 → src/components/icons.tsx\n`);
