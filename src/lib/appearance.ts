/**
 * 外观状态：主题（dark / light）与密度（compact / loose）。
 *
 * 设计约束（DESIGN.md §3 / §8.1）：
 *   - 默认主题 = **dark**（相片软件的行业惯例）
 *   - 主题**不跟随系统**，由用户手动切换
 *   - 密度只有两档，切换方式是 `<html data-theme data-density>`，
 *     **禁止**用 `transform: scale()`
 *
 * 这里只做两件事：读写持久化值 + 落到 DOM 上。
 * 界面（标题栏的两个分段控件）属于 M1-4，本文件先被陈列室用上。
 *
 * 存储用 `localStorage` 而不是 `app.db`：外观是**设备级**偏好（换台机器该重来一次），
 * 而且要在 React/Solid 首次渲染前就能拿到，走 IPC 进 DB 反而会闪一帧。
 */

export type ThemeMode = "dark" | "light";
export type DensityMode = "compact" | "loose";

export interface Appearance {
  theme: ThemeMode;
  density: DensityMode;
}

export const DEFAULT_APPEARANCE: Appearance = { theme: "dark", density: "compact" };

export const THEME_STORAGE_KEY = "raybend.theme";
export const DENSITY_STORAGE_KEY = "raybend.density";

/** 存储的最小接口（便于测试注入；也兼容 localStorage 被禁用时的静默失败） */
export interface AppearanceStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

function defaultStorage(): AppearanceStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    // 隐私模式 / 禁用存储时访问 localStorage 会抛错，那时就当作没有存储
    return undefined;
  }
}

/** 非法 / 缺失值一律回落到默认值 —— 存储里的垃圾不能让界面起不来 */
export function normalizeTheme(value: unknown): ThemeMode {
  return value === "light" || value === "dark" ? value : DEFAULT_APPEARANCE.theme;
}

export function normalizeDensity(value: unknown): DensityMode {
  return value === "compact" || value === "loose"
    ? value
    : DEFAULT_APPEARANCE.density;
}

export function readAppearance(
  storage: AppearanceStorage | undefined = defaultStorage(),
): Appearance {
  if (!storage) return { ...DEFAULT_APPEARANCE };
  try {
    return {
      theme: normalizeTheme(storage.getItem(THEME_STORAGE_KEY)),
      density: normalizeDensity(storage.getItem(DENSITY_STORAGE_KEY)),
    };
  } catch {
    return { ...DEFAULT_APPEARANCE };
  }
}

export function writeAppearance(
  appearance: Appearance,
  storage: AppearanceStorage | undefined = defaultStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(THEME_STORAGE_KEY, appearance.theme);
    storage.setItem(DENSITY_STORAGE_KEY, appearance.density);
  } catch {
    // 存不进去不影响使用，静默忽略
  }
}

/** 落到 `<html>` 上，令牌选择器（`[data-theme]` / `[data-density]`）由此生效 */
export function applyAppearance(appearance: Appearance): void {
  const root = globalThis.document?.documentElement;
  if (!root) return;
  root.dataset.theme = appearance.theme;
  root.dataset.density = appearance.density;
}
