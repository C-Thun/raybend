/**
 * 更新偏好与更新源（v2，崔总 2026-10-15 定）。
 *
 * 面向普通用户只留**稳定 / 预览**两个源（预览先灰着留口），界面里不出现密钥与自定义地址 ——
 * 「自定义源 + 换公钥」是留给第三方构建的能力，留在源码层（Rust 侧 `UpdateSource.mode = "custom"`
 * 照旧接受，只是没有 UI 入口）。
 *
 * 规格：`specs/update-flow-v2.md`。
 */
export type UpdateChannel = "stable" | "beta";
/** 传给 Rust 的源形状（对应 `src-tauri/src/updates/mod.rs` 的 `UpdateSource`）。 */
export interface UpdateSource {
  mode: UpdateChannel | "custom";
  endpoint: string;
  publicKey: string;
}
/** 用户能改的全部更新偏好。 */
export interface UpdatePrefs {
  channel: UpdateChannel;
  autoCheck: boolean;
}

/** v2 偏好键（`{channel, autoCheck}`）。 */
export const UPDATE_PREF_KEY = "raybend.updates.v2";
/** 旧键：只在一次性迁移时读一次，之后不再写。 */
export const UPDATE_KEY_V1 = "raybend.updates.v1";
export const AUTO_UPDATE_KEY_V1 = "raybend.updates.auto.v1";
/** 启动后延迟多久做那次静默动作（毫秒）：先让首屏起来，不抢启动时的带宽与主线程。 */
export const AUTO_UPDATE_DELAY_MS = 8000;
export const UPDATE_ENDPOINTS: Record<UpdateChannel, string> = {
  stable: "https://github.com/C-Thun/raybend/releases/latest/download/latest.json",
  beta: "https://raybend.cthun.com/updates/beta.json",
};
/** 预览通道**先灰着留口**：等真的有预览发布流程了再打开。 */
export const BETA_CHANNEL_ENABLED = false;

/**
 * 读偏好（含一次性迁移）。
 *
 * 迁移规则：v1 的 `off` 等价于「关掉自动检查」（新模型里没有「关闭」这个源）；
 * `custom` 一律归并到稳定源（能力留在源码层，UI 不再暴露）。
 */
export function readUpdatePrefs(
  v2: string | null,
  v1: string | null,
  autoV1: string | null,
): UpdatePrefs {
  if (v2) {
    try {
      const parsed = JSON.parse(v2) as { channel?: unknown; autoCheck?: unknown } | null;
      return {
        channel: parsed?.channel === "beta" ? "beta" : "stable",
        autoCheck: parsed?.autoCheck !== false,
      };
    } catch {
      /* 坏了就当没存过，往下走迁移。 */
    }
  }
  let legacyMode: string | null = null;
  try {
    const parsed = JSON.parse(v1 ?? "null") as { mode?: unknown } | null;
    legacyMode = typeof parsed?.mode === "string" ? parsed.mode : null;
  } catch {
    legacyMode = null;
  }
  return {
    channel: legacyMode === "beta" ? "beta" : "stable",
    autoCheck: autoV1 !== "off" && legacyMode !== "off",
  };
}

export function writeUpdatePrefs(prefs: UpdatePrefs): void {
  localStorage.setItem(UPDATE_PREF_KEY, JSON.stringify(prefs));
}

/** 构建是否带签名公钥：test/dev 构建里是空串 ⇒ 更新功能整体不可用（不联网、不可点）。 */
export function updatesAvailable(publicKey: string): boolean {
  return publicKey.trim().length > 0;
}

/**
 * 该不该做启动静默检查：构建通道要对得上（test/dev 不自动查，免得调试包被拉到正式版）、
 * 用户没关、构建带公钥。
 */
export function autoCheckAllowed(
  prefs: UpdatePrefs,
  buildChannel: string,
  publicKey: string,
): boolean {
  if (!prefs.autoCheck || !updatesAvailable(publicKey)) return false;
  if (buildChannel === "release") return prefs.channel === "stable";
  if (buildChannel === "beta") return true;
  return false;
}

/** 组装给 Rust 的源。 */
export function updateSource(prefs: UpdatePrefs, publicKey: string): UpdateSource {
  return { mode: prefs.channel, endpoint: UPDATE_ENDPOINTS[prefs.channel], publicKey };
}
