import { RELEASES_URL, REPO_URL } from './site.ts';

/**
 * 下载信息。
 *
 * **不在浏览器里请求 GitHub API** —— 版本与下载地址是**构建期**由 `vite.config.ts`
 * 注入的常量（`import.meta.env.RB_RELEASE`）：无请求、无限流、无闪烁。
 *
 * 注入的值来自两个渠道（优先级从高到低）：
 * 1. 发布流程给的 `RAYBEND_TAG`（`github.event.release.tag_name`，形如 `v0.1.0`）；
 * 2. CI 里查 GitHub API 的 `releases/latest`（只取正式版）。
 *
 * 还没有任何发布时注入 `null`，页面走 **pending** 态（按钮指向发布列表页）。
 * 这里的选择逻辑是纯函数，单测在 `release.test.ts`。
 */

/** 注入进构建的最小发布信息（与 `vite.config.ts` 的注入结构一致） */
export interface ReleaseInfo {
  /** Git tag，形如 `v0.1.0` */
  tag: string;
  /** 发布页地址（缺省时按 tag 拼） */
  url?: string;
  /** 发布时间（ISO 8601） */
  publishedAt?: string;
  /** 是否为预发布版（正式版才会上官网） */
  prerelease?: boolean;
  /** 草稿永不展示 */
  draft?: boolean;
  /** 发布资产（安装包等） */
  assets?: readonly ReleaseAsset[];
}

export interface ReleaseAsset {
  name: string;
  /** 字节数 */
  size?: number;
  /** 直链（GitHub 的 `browser_download_url`） */
  url?: string;
}

export type ReleaseView =
  | {
      state: 'pending';
      /** 还没有发布时按钮的去处：发布列表页 */
      releaseUrl: string;
    }
  | {
      state: 'available';
      /** 去掉前导 `v` 的版本号，例如 `0.1.0` */
      version: string;
      tag: string;
      releaseUrl: string;
      /** 安装包直链；没有可识别资产时退化为发布页 */
      downloadUrl: string;
      /** 有直链时才有文件名与体积 */
      fileName?: string;
      /** 人类可读体积，例如 `12.4 MB` */
      sizeLabel?: string;
      /** 发布时间（ISO），显示格式交给组件按语言处理 */
      publishedAt?: string;
    };

/** 安装包优先级：NSIS 安装包 → MSI；签名/校验文件不算资产 */
const ASSET_PREFERENCE: readonly (readonly [string, readonly string[]])[] = [
  ['exe', ['.exe']],
  ['msi', ['.msi']],
];

/** 校验 tag 合法性：必须像 `v1.2.3` 或 `1.2.3(-beta.1)`，别的当没发布处理 */
export function parseTag(tag: string | undefined | null): string | undefined {
  if (typeof tag !== 'string') return undefined;
  const trimmed = tag.trim();
  const match = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/.exec(trimmed);
  if (!match || match.slice(1, 4).some(v => !Number.isSafeInteger(Number(v))) || match[4]?.split('.').some(v => /^0\d+$/.test(v))) return undefined;
  return trimmed;
}

/** `12345678` → `12.3 MB` */
export function formatBytes(bytes: number | undefined): string | undefined {
  if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes <= 0) return undefined;
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

/** 从发布资产里挑一个安装包（挑不到就返回 undefined） */
export function pickInstaller(
  assets: readonly ReleaseAsset[] | undefined,
): ReleaseAsset | undefined {
  if (!assets?.length) return undefined;
  for (const [, extensions] of ASSET_PREFERENCE) {
    for (const asset of assets) {
      if (!asset.url || !/^RayBend_.+_x64(?:-setup\.exe|(?:_[A-Za-z-]+)?\.msi)$/i.test(asset.name)) continue;
      try { const url=new URL(asset.url); if(url.protocol!=='https:' || url.hostname!=='github.com' || url.username || url.password || url.search || url.hash || !url.pathname.startsWith('/C-Thun/raybend/releases/download/') || decodeURIComponent(url.pathname.split('/').at(-1) ?? '')!==asset.name)continue; } catch { continue; }
      const lower = asset.name.toLowerCase();
      if (extensions.some((ext) => lower.endsWith(ext))) return asset;
    }
  }
  return undefined;
}

/**
 * 把注入的发布信息算成页面要显示的下载状态。
 *
 * - 没有信息 / tag 不合法 / 预发布版 → `pending`（正式版才上官网）
 * - 有正式版：优先给安装包直链，没有可识别资产就给发布页
 */
export function resolveRelease(info: ReleaseInfo | null | undefined): ReleaseView {
  const tag = parseTag(info?.tag);

  if (!info || !tag || info.prerelease || info.draft || tag.split('+')[0]!.includes('-')) {
    return { state: 'pending', releaseUrl: RELEASES_URL };
  }

  const version = tag.replace(/^v/i, '');
  const releaseUrl = `${REPO_URL}/releases/tag/${encodeURIComponent(tag)}`;
  const installer = pickInstaller(info.assets?.filter(asset => { try { return decodeURIComponent(new URL(asset.url ?? '').pathname.split('/').at(-2) ?? '')===tag && asset.name.startsWith(`RayBend_${version}_`); } catch { return false; } }));

  return {
    state: 'available',
    version,
    tag,
    releaseUrl,
    downloadUrl: installer?.url ?? releaseUrl,
    fileName: installer?.name,
    sizeLabel: formatBytes(installer?.size),
    publishedAt: info.publishedAt,
  };
}

/** 构建期注入的发布信息（本地 dev 没注入时返回 `null`） */
export function injectedRelease(): ReleaseInfo | null {
  const injected = import.meta.env.RB_RELEASE;
  return injected && typeof injected === 'object' ? injected : null;
}

/** 页面直接用的下载状态 */
export function currentRelease(): ReleaseView {
  return resolveRelease(injectedRelease());
}
