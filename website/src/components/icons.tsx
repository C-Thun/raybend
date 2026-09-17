/**
 * 本站用到的 Lucide 图标。
 *
 * ⚠️ **本文件由 `scripts/generate-icons.mjs` 生成，不要手改** ——
 * 要加图标就往那个脚本的清单里加一行，再跑 `pnpm icons:generate`。
 *
 * 只取 Lucide 的**图标数据**（ISC 许可，见 `THIRD-PARTY-NOTICES.md`），组件本身是本站写的：
 * `lucide-solid` 1.x 依赖 Solid 1 的 `splitProps`，在 Solid 2 下无法使用。
 */
import type { JSX } from '@solidjs/web';

export interface IconProps {
  /** 尺寸与颜色走类名（如 `size-5 text-brand-dark`）；接受 Solid 2 的字符串/数组/对象写法 */
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

/** `zap` — https://lucide.dev/icons/zap */
export function IconBolt(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M15.914 4a1.5 1.5 0 00-2.474-1.561l-9 9A1.5 1.5 0 005.5 14h4.002a.5.5 0 01.471.666L8.086 20a1.5 1.5 0 002.475 1.56l9-9A1.5 1.5 0 0018.5 10h-3.997a.5.5 0 01-.472-.667z" />
    </IconBase>
  );
}

/** `circle-check` — https://lucide.dev/icons/circle-check */
export function IconCheckCircle(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <circle cx="12" cy="12" r="10" />
      <path d="m16 9-5.5 5.5L8 12" />
    </IconBase>
  );
}

/** `cpu` — https://lucide.dev/icons/cpu */
export function IconChip(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M12 20v2" />
      <path d="M12 2v2" />
      <path d="M17 20v2" />
      <path d="M17 2v2" />
      <path d="M2 12h2" />
      <path d="M2 17h2" />
      <path d="M2 7h2" />
      <path d="M20 12h2" />
      <path d="M20 17h2" />
      <path d="M20 7h2" />
      <path d="M7 20v2" />
      <path d="M7 2v2" />
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <rect x="8" y="8" width="8" height="8" rx="1" />
    </IconBase>
  );
}

/** `download` — https://lucide.dev/icons/download */
export function IconDownload(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M12 15V3" />
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <path d="m7 10 5 5 5-5" />
    </IconBase>
  );
}

/** `folder-input` — https://lucide.dev/icons/folder-input */
export function IconFolderInput(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M2 9V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-1" />
      <path d="M2 13h10" />
      <path d="m9 16 3-3-3-3" />
    </IconBase>
  );
}

/** `layout-grid` — https://lucide.dev/icons/layout-grid */
export function IconGrid(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <rect width="7" height="7" x="3" y="3" rx="1" />
      <rect width="7" height="7" x="14" y="3" rx="1" />
      <rect width="7" height="7" x="14" y="14" rx="1" />
      <rect width="7" height="7" x="3" y="14" rx="1" />
    </IconBase>
  );
}

/** `hard-drive` — https://lucide.dev/icons/hard-drive */
export function IconHardDrive(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M10 16h.01" />
      <path d="M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
      <path d="M21.946 12.013H2.054" />
      <path d="M6 16h.01" />
    </IconBase>
  );
}

/** `image` — https://lucide.dev/icons/image */
export function IconImage(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
      <circle cx="9" cy="9" r="2" />
      <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
    </IconBase>
  );
}

/** `lock` — https://lucide.dev/icons/lock */
export function IconLock(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <rect width="18" height="11" x="3" y="11" rx="2" ry="2" />
      <path d="M7 11V7a5 5 0 0 1 10 0v4" />
    </IconBase>
  );
}

/** `monitor-down` — https://lucide.dev/icons/monitor-down */
export function IconMonitorDown(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M12 13V7" />
      <path d="m15 10-3 3-3-3" />
      <rect width="20" height="14" x="2" y="3" rx="2" />
      <path d="M12 17v4" />
      <path d="M8 21h8" />
    </IconBase>
  );
}

/** `circle-play` — https://lucide.dev/icons/circle-play */
export function IconPlayCircle(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M9 9.003a1 1 0 0 1 1.517-.859l4.997 2.997a1 1 0 0 1 0 1.718l-4.997 2.997A1 1 0 0 1 9 14.996z" />
      <circle cx="12" cy="12" r="10" />
    </IconBase>
  );
}

/** `shield-check` — https://lucide.dev/icons/shield-check */
export function IconShieldCheck(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />
      <path d="m9 12 2 2 4-4" />
    </IconBase>
  );
}

/** `tags` — https://lucide.dev/icons/tags */
export function IconTags(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M13.172 2a2 2 0 0 1 1.414.586l6.71 6.71a2.4 2.4 0 0 1 0 3.408l-4.592 4.592a2.4 2.4 0 0 1-3.408 0l-6.71-6.71A2 2 0 0 1 6 9.172V3a1 1 0 0 1 1-1z" />
      <path d="M2 7v6.172a2 2 0 0 0 .586 1.414l6.71 6.71a2.4 2.4 0 0 0 3.191.193" />
      <circle cx="10.5" cy="6.5" r=".5" fill="currentColor" />
    </IconBase>
  );
}

/** `git-branch` — https://lucide.dev/icons/git-branch */
export function IconTree(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M15 6a9 9 0 0 0-9 9V3" />
      <circle cx="18" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
    </IconBase>
  );
}

/** `upload` — https://lucide.dev/icons/upload */
export function IconUpload(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="M12 3v12" />
      <path d="m17 8-5-5-5 5" />
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    </IconBase>
  );
}

/** `video` — https://lucide.dev/icons/video */
export function IconVideo(props: IconProps) {
  return (
    <IconBase class={props.class} strokeWidth={props.strokeWidth}>
      <path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5" />
      <rect x="2" y="6" width="14" height="12" rx="2" />
    </IconBase>
  );
}
