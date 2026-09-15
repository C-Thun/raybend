/**
 * 构建信息（版本 / 通道 / 构建时间 / git 状态）。
 *
 * 为什么需要它：`关于` 弹窗要显示版本与「这是测试包还是正式包」，
 * 而「支持时」还要能一眼看出用户装的是哪个 commit。这些值**只有构建时才知道**，
 * 所以由 `vite.config.ts` 用 `define` 注入。
 *
 * 分层上的考虑（这也是本文件为什么这样切）：
 *   - `readBuildInfo()` 是**纯函数**（吃注入值、吐结构化信息），所以能被单元测试覆盖，
 *     包括「注入值是垃圾」这种真实会发生的场景（打包脚本传错环境变量）
 *   - `currentBuildInfo()` 才去碰注入的全局量。测试不调用它，
 *     所以 Node 环境下没有注入也不会炸
 */

/** 构建通道：`dev` 本地开发 / `test` 测试包 / `beta` 公测 / `release` 正式版 */
export const BUILD_CHANNELS = ["dev", "test", "beta", "release"] as const;
export type BuildChannel = (typeof BUILD_CHANNELS)[number];

export interface BuildInfo {
 version: string;
 channel: BuildChannel;
 /** 构建时刻（ISO 8601）。未知时为空串，界面按「不显示」处理 */
 builtAt: string;
 /** 短 commit hash */
 gitHash?: string;
 /** 构建时工作树是否脏（有未提交改动）—— 正式包不该是脏的 */
 dirty?: boolean;
}

export const UNKNOWN_BUILD: BuildInfo = {
 version: "0.0.0",
 channel: "dev",
 builtAt: "",
};

function isChannel(value: unknown): value is BuildChannel {
 return (
  typeof value === "string" &&
  (BUILD_CHANNELS as readonly string[]).includes(value)
 );
}

/** 从任意注入值里取出可用的构建信息；缺什么补什么，绝不抛错 */
export function readBuildInfo(raw: unknown): BuildInfo {
 const record: Record<string, unknown> =
  raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};

 const rawVersion = record.version;
 const version =
  typeof rawVersion === "string" && rawVersion.trim().length > 0
   ? rawVersion.trim()
   : UNKNOWN_BUILD.version;

 const rawHash = record.gitHash;

 return {
  version,
  channel: isChannel(record.channel) ? record.channel : UNKNOWN_BUILD.channel,
  builtAt: typeof record.builtAt === "string" ? record.builtAt : "",
  gitHash:
   typeof rawHash === "string" && rawHash.length > 0 ? rawHash : undefined,
  dirty: record.dirty === true,
 };
}

/** 读取构建时注入的信息（生产/开发构建里一定存在；Node 测试里不会调用它） */
export function currentBuildInfo(): BuildInfo {
 return readBuildInfo(
  typeof __RAYBEND_BUILD__ === "undefined" ? undefined : __RAYBEND_BUILD__,
 );
}

/** 通道的显示名（走 i18n 的调用方可以不用它；这是给调试信息用的英文短名） */
export function channelLabel(channel: BuildChannel): string {
 switch (channel) {
  case "release":
   return "Release";
  case "beta":
   return "Beta";
  case "test":
   return "Test";
  case "dev":
   return "Dev";
 }
}

/** 是否有资格当作正式发布（正式包必须是 release 且工作树干净） */
export function isReleaseReady(info: BuildInfo): boolean {
 return info.channel === "release" && info.dirty !== true;
}

/** 形如 `0.1.0 · Dev · 3f2a1c9`；构建时间未知时省略 */
export function formatBuildLabel(info: BuildInfo): string {
 const parts = [info.version, channelLabel(info.channel)];
 if (info.gitHash) parts.push(info.gitHash);
 return parts.join(" · ");
}

/**
 * 要不要显示调试信息（commit、构建时间、WebView 版本）。
 *
 * 判定依据是**构建通道**而不是 UI 的开关：正式包的用户没必要看 commit hash，
 * 而支持人员调一个测试包时必须要看 —— 两边的条件不一样，不能共用一个「显示详情」的按钮。
 */
export function showsDebugInfo(channel: BuildChannel): boolean {
 return channel === "dev" || channel === "test";
}

declare global {
 // 由 vite.config.ts 的 `define` 注入（见那里的注释）
 var __RAYBEND_BUILD__: unknown;
}
