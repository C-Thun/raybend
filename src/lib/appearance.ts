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

import { createSignal } from "solid-js";

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

/**
 * 开发期用 URL 覆盖外观：`?theme=light&density=loose`。
 *
 * 为什么值得为它写代码：视觉自查要反复切「两主题 × 两密度」，而主题/密度存在
 * localStorage（设备级偏好）—— 靠脚本改浏览器 profile 既脆弱又慢。
 * 给一个 URL 覆盖，截图脚本（`pnpm shot`）与人**都能一行切到想要的形态**。
 *
 * 纯函数：吃 query string、吐部分覆盖值；非法值忽略，不报错。
 * **只在 `import.meta.env.DEV` 下被调用**，生产构建不读 URL。
 */
export function readAppearanceOverride(search: string): Partial<Appearance> {
  const params = new URLSearchParams(search.startsWith("?") ? search : `?${search}`);
  const override: Partial<Appearance> = {};

  const theme = params.get("theme");
  if (theme === "dark" || theme === "light") override.theme = theme;

  const density = params.get("density");
  if (density === "compact" || density === "loose") override.density = density;

  return override;
}

/* ══════════════════════════════════════════════════════════════
 * 响应式外观状态（titlebar 的开关与陈列室都用它）
 * ══════════════════════════════════════════════════════════════ */

export interface AppearanceStore {
  theme: () => ThemeMode;
  density: () => DensityMode;
  appearance: () => Appearance;
  setTheme: (theme: ThemeMode) => void;
  setDensity: (density: DensityMode) => void;
  /** 深↔浅切换（titlebar 的主题按钮就是它） */
  toggleTheme: () => void;
}

export interface AppearanceStoreOptions {
  /** 初始值；省略时从存储读（读不到就是默认） */
  initial?: Partial<Appearance>;
  /** 注入存储（测试用） */
  storage?: AppearanceStorage | undefined;
  /** 注入「落到 DOM」的动作（测试用；Node 环境里没有 document） */
  apply?: (appearance: Appearance) => void;
}

/**
 * 创建外观状态：改一下就**同时**持久化并落到 `<html>` 上。
 *
 * 为什么把「读、写、落 DOM」绑在一起：分散在三处写就会出现
 * 「切了主题但没存」或「存了但没生效」这种只能靠刷新才发现的偏差。
 */
export function createAppearanceStore(
  options: AppearanceStoreOptions = {},
): AppearanceStore {
  const storage = "storage" in options ? options.storage : defaultStorage();
  const apply = options.apply ?? applyAppearance;
  const persisted = readAppearance(storage);
  /*
   * 开发期允许 URL 覆盖（`?theme=light&density=loose`），见 readAppearanceOverride。
   *
   * `import.meta.env?.DEV` 的 `?.` 不是摆设：单测在 Node 里跑，那里的
   * `import.meta.env` 是 undefined —— 写死 `.DEV` 会让整组测试直接抛错。
   */
  const isDev = import.meta.env?.DEV === true;
  const search = globalThis.location?.search ?? "";
  const fromUrl: Partial<Appearance> = isDev ? readAppearanceOverride(search) : {};

  const [theme, setThemeSignal] = createSignal<ThemeMode>(
    options.initial?.theme ?? fromUrl.theme ?? persisted.theme,
  );
  const [density, setDensitySignal] = createSignal<DensityMode>(
    options.initial?.density ?? fromUrl.density ?? persisted.density,
  );

  const appearance = (): Appearance => ({ theme: theme(), density: density() });

  const commit = (next: Appearance): void => {
    apply(next);
    writeAppearance(next, storage);
  };

  // 首次就把当前值落下去：`<html>` 上的属性必须与 store 一致，
  // 否则首帧会按默认主题画，切一次才能对上。
  commit(appearance());

  return {
    theme,
    density,
    appearance,
    setTheme: (next) => {
      setThemeSignal(normalizeTheme(next));
      commit(appearance());
    },
    setDensity: (next) => {
      setDensitySignal(normalizeDensity(next));
      commit(appearance());
    },
    toggleTheme: () => {
      setThemeSignal(theme() === "dark" ? "light" : "dark");
      commit(appearance());
    },
  };
}
