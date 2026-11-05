/**
 * 编辑视口的 IPC 封装（`src/` 里只有本目录可以直接 `invoke`，见 `memory/ARCHITECTURE.md` §1.1）。
 *
 * 契约的两条纪律：
 *
 * 1. **前端只上报原始事实**（CSS 矩形 + 运行时 DPR + CSS 视口尺寸 + 计算出来的底色字符串）——
 *    物理换算、缩放、命中测试全在 Rust（`AGENTS.md` §6.1 红线 2、§7.9 铁律 2）；
 * 2. **非法值当面报错**，不静默回退 —— Rust 侧会拒绝 NaN / 非正 DPR，
 *    这一层把错误往上抛给调用方（上报器有自己的 try 边界）。
 *
 * M3-W1 只有「上报 + 回读」两条命令；M3-W2 加上渲染线程那四条：
 * `bind`（懒启动）/ `unbind` / `setPhoto` / `intent`，外加一条轮询用的 `getRenderState`。
 * **照片像素不出 Rust** —— 前端拿到的只有状态（哪张、什么档位、画了没有），
 * 图是 wgpu 直接画到窗口上的。
 */

import type {
  DevelopEditBase,
  DevelopEditTarget,
  EditorRenderState,
  EditorViewportIntent,
  EditorViewportState,
  DevelopParamsPayload,
  DevelopSettings,
  LensMatch,
  EditGeometry,
} from "./types.ts";
import type { EditorViewportPayload } from "../lib/editor-viewport.ts";
import { overlayPulse, publishOverlayLive } from "../lib/overlay-live.ts";
import { isTauriRuntime } from "./tauri-env.ts";

/** 缓存的 `@tauri-apps/api/core` 模块（浏览器里根本不会加载它）。 */
let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/**
 * 上报洞口（CSS 像素）+ DPR + 视口尺寸 + 洞口底色。
 *
 * 浏览器（无 Tauri）里返回 `null` —— 那里没有渲染线程可喂，不该报错。
 * 返回 `null` 也表示这条命令在当前环境不可用，调用方据此跳过后续动作。
 */
export async function setEditorViewport(
  payload: EditorViewportPayload,
): Promise<EditorViewportState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorViewportState>("editor_set_viewport", { ...payload });
}

/** 读回 Rust 手里的视口事实（诊断 / 冒烟）。 */
export async function getEditorViewportState(): Promise<EditorViewportState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorViewportState>("editor_viewport_state");
}

/**
 * 起渲染线程（幂等）。编辑器挂载时调一次。
 *
 * 浏览器里返回 `null`（那里没有窗口与 GPU 表面可挂）；调用方据此把洞口留在 DOM 态
 * （水印/占位照旧显示），**不**进入「透明洞口」那条路。
 */
export async function bindEditorRenderer(): Promise<EditorRenderState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorRenderState>("editor_bind_renderer");
}

/** 停渲染线程（幂等）。离开编辑器时调。 */
export async function unbindEditorRenderer(): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("editor_unbind_renderer");
}

/** 换照片（`null` = 清空）。前端在锚点变化时调。 */
export async function setEditorPhoto(path: string | null): Promise<EditorRenderState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorRenderState>("editor_set_photo", { path });
}

/** 发一条视口意图（缩放 / 平移 / 档位 / 复位 / 命中测试）。 */
export async function sendEditorViewportIntent(
  intent: EditorViewportIntent,
): Promise<void> {
  if (!isTauriRuntime()) return;
  // 视口要动了：覆盖层开始每帧读变换（否则照片动了、标记还停在原地）
  overlayPulse();
  return call<void>("editor_viewport_intent", { intent });
}

/**
 * **开发期**：控制点容差映射的固化默认值（`develop/areas.rs::POINT_WEIGHT_CONFIG`）。
 *
 * 只给 `dev/MaskDebugDialog` 的「恢复默认」用 —— 前端不抄一份数字，避免漂。
 */
export async function getMaskDefaults(): Promise<import("../lib/mask-debug.ts").MaskWeightConfig> {
  const fallback: import("../lib/mask-debug.ts").MaskWeightConfig = {
    lumaCore: [0, 0], lumaEnd: [0, 0], chromaCore: [0, 0], chromaEnd: [0, 0],
  };
  if (!isTauriRuntime()) return fallback;
  /*
   * **在 I/O 边界解码**（不是把 `undefined` 直接当成配置用）：这条命令只在开发期调试对话框里用，
   * 但后端没实现（旧版 / 冒烟 fixture）时会返回 `undefined` —— 直接透出去会让对话框在
   * `defaults.lumaCore` 上抛未捕获异常（2026-11-05 `check:export` 的 exceptions 断言就是它）。
   * 形状不对就退回零配置：调试对话框能开、能写值，只是默认值全 0。
   */
  const raw = await call<Partial<import("../lib/mask-debug.ts").MaskWeightConfig> | null>("editor_mask_defaults");
  const pair = (value: unknown): [number, number] | null => {
    if (!Array.isArray(value) || value.length !== 2) return null;
    const [a, b] = value;
    return typeof a === "number" && typeof b === "number" && Number.isFinite(a) && Number.isFinite(b)
      ? [a, b] : null;
  };
  const lumaCore = pair(raw?.lumaCore);
  const lumaEnd = pair(raw?.lumaEnd);
  const chromaCore = pair(raw?.chromaCore);
  const chromaEnd = pair(raw?.chromaEnd);
  if (lumaCore === null || lumaEnd === null || chromaCore === null || chromaEnd === null) return fallback;
  return { lumaCore, lumaEnd, chromaCore, chromaEnd };
}

/** 覆盖层的实时变换（只取矩阵 / 比例 / 旋转 / 照片身份，手势期间每帧一次）。 */
export async function getEditorOverlayState(): Promise<import("../lib/overlay-live.ts").OverlayLive | null> {
  if (!isTauriRuntime()) return null;
  const state = await call<import("../lib/overlay-live.ts").OverlayLive>("editor_overlay_state");
  publishOverlayLive(state);
  return state;
}

/** 命名定稿对比参照：Rust 读独立 AVIF 快照并解码，序号挡住迟到结果。 */
export async function setEditorReferenceIssue(
  repositoryId: string, assetId: number, issueId: number, photoPath: string, sequence: number,
): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("editor_set_reference_issue", { repositoryId, assetId, issueId, photoPath, sequence });
}

/**
 * **邻图预览预热**（W2 §4.6）：把胶片带前后各一张的 1920 预览在后台（P2）备好，
 * 来回切换时首帧直接命中 L1 快照。只对编辑过且缺快照的邻居动手；
 * 新一次调用会撤掉上一次还没开跑的预热。
 */
export async function prefetchEditorNeighbors(paths: readonly string[]): Promise<void> {
  if (!isTauriRuntime() || paths.length === 0) return;
  await call<void>("editor_prefetch_neighbors", { paths: [...paths] });
}

/** 读取 Rust 工具草稿；只有确认按钮会将它写入编辑栈。 */
export async function confirmEditorTool(): Promise<EditGeometry | null> {
  if (!isTauriRuntime()) return null;
  return call<EditGeometry>("editor_confirm_tool");
}

/**
 * **显影参数**（拉杆 / 曲线）—— 参数一变就发，渲染线程重算像素。
 *
 * 拖动中每帧一条完全没问题：渲染线程按「最新者优先」丢弃过期任务（`editor.rs` 的
 * `develop_loop`），而且管线跑在**显影线程**上，渲染线程永远只管画上一张。
 *
 * 浏览器里返回 `null`（没有渲染线程可喂）。
 */
export async function setEditorParams(
  repositoryId: string | null,
  assetId: number | null,
  params: DevelopParamsPayload,
): Promise<EditorRenderState | null> {
  if (!isTauriRuntime()) return null;
  // 后端要拿这两个去读**这张照片的拍摄参数**（焦距 / 光圈 / 机身）才能解析镜头配置
  // ——对比起让前端再传一遍（同一份事实两处传，早晚对不上）。
  return call<EditorRenderState>("editor_set_params", { repositoryId, assetId, params });
}

/**
 * 读渲染线程的状态（前端每 250ms 一次）。
 *
 * 它同时干两件事：**握手**（`ready` + `paintedPath` 决定洞口那条 DOM 链要不要透明）
 * 与**上报**（`restarts` / `lastError`：渲染线程崩过但爬起来了，界面必须能看见）。
 */
export async function getEditorRenderState(): Promise<EditorRenderState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorRenderState>("editor_render_state");
}

/* ══════════════════════════════════════════════════════════════
 * 编辑栈（`latest`）—— M3-W3 的落库口
 * ══════════════════════════════════════════════════════════════ */

/** 一张照片的编辑栈（与 Rust 侧 `DevelopStackDto` 逐字对应）。 */
/**
 * 应用层的一层（多层 LUT）：哪一支、开没开、强度多少。
 *
 * 语义（`specs/editor-lut-chain.md` §1）：`像素 = layer_n(...(layer_1(原像素)))`，
 * 单层作用 = `mix(输入, LUT(输入), strength)`。
 */
export interface LutLayer {
  id: string;
  enabled: boolean;
  /** 0..1（1 = 100%） */
  strength: number;
}

/**
 * 编辑区（area）的一层（与 Rust 侧 `store::develop::Area` 逐字对应）。
 *
 * `type` 用 serde 的 `tag = "type"` flatten：`full` 只有公共字段，
 * `point` 多出采样位 / 种子 / 容差（`specs/editor-area.md` §3.1）。
 */
export interface AreaCommon {
  id: number;
  /** 用户起的名字（≤30 字符）。空 = 用类型默认名。 */
  name: string;
  enabled: boolean;
  /** 0..1（近期没有界面） */
  opacity: number;
  /** 控制点的颜色槽位（0..8）；全图层不画标记 */
  color: number;
  params: Record<string, number>;
  curves: Record<string, [number, number][]>;
  /**
   * 控制点种子的定义版本（与 Rust `store::develop::SEED_VERSION` 一致）。
   *
   * `0` / 缺省 = 旧定义（线性域）—— 前端载入时会**逐点重新取样**，位置不变。
   */
  seedVersion: number;
}

export type Area = AreaCommon &
  (
    | { type: "full" }
    | {
      type: "point";
      /** 采样位：图像归一化坐标（方向已归正、裁切前） */
      x: number;
      y: number;
      /** 创建时冻结的特征向量 `[luma, c1, c2]`（0..1） */
      seed: [number, number, number];
      /** 归一化向量距离的尺度（0..1）——**不是空间半径** */
      tolerance: number;
    }
  );

export interface DevelopStack {
  color?: import("./color.ts").PhotoColorState | null;
  autoAdjust?: import("./types.ts").AutoAdjustBaseline | null;
  /** latest 的唯一源；其它调整参数在 SOOC/RAW 切换时共用。 */
  sourceBase?: DevelopEditBase;
  /** 参数 id → 值（**只装与基线不同的项**） */
  values: Record<string, number>;
  /** 通道 → 控制点（归一化 0..1）；只装动过的通道 */
  curves: Record<string, [number, number][]>;
  /**
   * 拍摄色温（K）—— 色温拉杆的基线，**跟着 issue 一起存**。
   *
   * 不存它的话，缩略图那条路（读不到 RAW 元数据）会用 6250 兜底，
   * 同一份参数就会渲染出两种颜色。
   */
  asShotK?: number | null;
  /**
   * 镜头配置文件（`null` = 未选择；`"none"` = 显式清除配置；否则是 `maker|model`）。
   *
   * 自动调整和手动选择均保存具体稳定键；`"none"` 保留为兼容的显式清除值。
   */
  lensProfile?: string | null;
  /** RAW 机型基础曲线的选择和像素快照。 */
  baseCurveProfile?: string | null;
  baseCurvePoints?: [number, number][] | null;
  /** 应用层链（有序；空 = 没有 LUT）。总开关是 `lutEnabled`。 */
  lutChain?: LutLayer[] | null;
  lutEnabled?: boolean | null;
  /** 配置文件那一半的开关（`null` = 默认开）。**手动三根拉杆不受它影响**。 */
  lensEnabled?: boolean | null;
  /** 降噪方式（`null` = 快速档；`"high"` = BM3D 高质量档）。 */
  nrMethod?: string | null;
  geometry?: EditGeometry | null;
  /** **默认层之外的编辑区层**（空 = 只有默认层）。 */
  areas?: Area[] | null;
  /** 默认层在层列表里的位置（0-based；只有存在额外层时才有意义）。 */
  defaultIndex?: number | null;
}

/** 编辑栈 → store 设置的唯一适配，撤销与普通载图共用。 */
export function developSettingsOf(stack: DevelopStack): DevelopSettings {
  return {
    color: stack.color ?? null,
    autoAdjust: stack.autoAdjust ?? null,
    sourceBase: stack.sourceBase ?? "raw",
    baseCurveProfile: stack.baseCurveProfile ?? null,
    baseCurvePoints: stack.baseCurvePoints ?? null,
    lutChain: stack.lutChain ?? null,
    lutEnabled: stack.lutEnabled ?? null,
    asShotK: stack.asShotK ?? null,
    lensProfile: stack.lensProfile ?? null,
    lensEnabled: stack.lensEnabled ?? null,
    nrMethod: stack.nrMethod === "high" ? "high" : null,
    geometry: stack.geometry ?? null,
    areas: stack.areas ?? null,
    defaultIndex: stack.defaultIndex ?? null,
  };
}

/** 按需读取这张照片的镜头匹配与全库候选；失败携带原因拒绝。 */
export async function getLensMatch(
  repositoryId: string,
  assetId: number,
): Promise<LensMatch | null> {
  if (!isTauriRuntime()) return null;
  return call<LensMatch>("lens_match", { repositoryId, assetId });
}

/** 落库 / 重置的结果：栈 + 撤销栈快照（界面据此显示「撤销：调整参数」）。 */
export interface DevelopCommitResult {
  stack: DevelopStack;
  /** 刚记进撤销栈的那一步叫什么（没改动就是 `null`） */
  undoLabel: string | null;
  canUndo: boolean;
}

/** 读这张照片的编辑栈（没有 = 空栈，不是错误）。 */
export async function getDevelopStack(
  repositoryId: string,
  assetId: number,
): Promise<DevelopStack | null> {
  if (!isTauriRuntime()) return null;
  return call<DevelopStack>("develop_get", { repositoryId, assetId });
}

/**
 * **落库**（覆盖式：载荷里没有的项 = 没动过 = 删掉）。
 *
 * 只在**松手**时调（拖动中每帧都写库会把单写者线程淹掉，也毫无意义）。
 */
export async function commitDevelopStack(
  repositoryId: string,
  assetId: number,
  stack: DevelopStack,
): Promise<DevelopCommitResult | null> {
  if (!isTauriRuntime()) return null;
  // 编辑栈整体作为**一个**参数发过去（不再把六个字段摊在命令参数上）——
  // 加一项设置时只改 DTO，不必再动命令签名
  const payload: DevelopStack = {
    color: stack.color ?? null,
    autoAdjust: stack.autoAdjust ?? null,
    sourceBase: stack.sourceBase ?? "raw",
    values: stack.values,
    curves: stack.curves,
    asShotK: stack.asShotK ?? null,
    lensProfile: stack.lensProfile ?? null,
    baseCurveProfile: stack.baseCurveProfile ?? null,
    baseCurvePoints: stack.baseCurvePoints ?? null,
    lutChain: stack.lutChain ?? null,
    lutEnabled: stack.lutEnabled ?? null,
    lensEnabled: stack.lensEnabled ?? null,
    nrMethod: stack.nrMethod ?? null,
    geometry: stack.geometry ?? null,
    areas: stack.areas ?? [],
    defaultIndex: stack.defaultIndex ?? null,
  };
  return call<DevelopCommitResult>("develop_commit", {
    repositoryId,
    assetId,
    stack: payload,
  });
}

/** 控制点落点的取样结果（`specs/editor-area.md` §3.3）。 */
export interface AreaSeed {
  /** 归一化坐标（0..1，显示画幅口径） */
  x: number;
  y: number;
  /** 创建时冻结的特征向量 `[luma, c1, c2]`（0..1） */
  seed: [number, number, number];
}

/**
 * **控制点落点取样**：给定归一化坐标，返回「该层输入」的特征向量。
 *
 * 取样由 Rust 做（解 1024 长边的线性源 → 把当前栈合成一遍 → 取那个像素）；
 * 前端只发坐标与当前的编辑栈 —— 种子怎么算只有一份实现。
 */
export async function sampleAreaSeed(
  repositoryId: string,
  assetId: number,
  x: number,
  y: number,
  stack: DevelopStack,
  /** 目标层在**行序**里的位置（`undefined` = 整链跑完；`0` = 原始源） */
  layer?: number,
): Promise<AreaSeed | null> {
  if (!isTauriRuntime()) return null;
  return call<AreaSeed>("develop_area_seed", {
    repositoryId,
    assetId,
    x,
    y,
    layer: layer ?? null,
    stack: {
      ...stack,
      values: stack.values,
      curves: stack.curves,
      areas: stack.areas ?? [],
      defaultIndex: stack.defaultIndex ?? null,
    },
  });
}

/**
 * **刷新 preview**（`memory/FUNCTION-IMAGING.md` §4）：编辑器**进 / 出**两个节点各调一次。
 *
 * preview = 库内大图缓存（长边 1920 的 AVIF，`<库根>/cache/full/…`）—— 与 `view_image`
 * 走的是同一份（命中只读、未命中才渲染）。没编辑过时 Rust 侧**直接跳过**：
 * SOOC / RAW 的内置位图就代替 preview。
 *
 * 它是**后台那一路**：调用方不必等它（返回值只用于诊断，`false` = 没生成，不是错误）。
 */
export async function refreshDevelopPreview(path: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  return call<boolean>("develop_preview_refresh", { path });
}

/**
 * **编辑器该编辑哪个文件**（「编辑落在 RAW 上」，`memory/FUNCTION-REPOSITORY.md` §4.1）。
 *
 * `base` 是总览图下那个切换按钮选的基准（人类 2026-09-24，**默认 `"raw"`**）：
 * `"raw"` 时返回 `_RAW/` 里那个 RAW 的绝对路径（没 RAW 就退回位图）；
 * `"sooc"` 时返回相机直出的位图（没位图就退回 RAW）。
 * 拼 `_RAW/` 这件事只在 Rust 侧实现一次 —— 前端不许自己拼。
 *
 * 返回里还带 `hasBitmap` / `hasRaw`：界面据此**禁用**切不过去的那一侧。
 * `path === null` = 没有可编辑的文件（资产缺文件 / 库离线）。
 */
export async function getDevelopEditTarget(
  repositoryId: string,
  assetId: number,
  base: DevelopEditBase = "raw",
): Promise<DevelopEditTarget | null> {
  if (!isTauriRuntime()) return null;
  return call<DevelopEditTarget>("develop_edit_target", { repositoryId, assetId, base });
}

/** 同机型基础曲线档案；没有品牌+型号时 camera 字段为 null。 */
export interface BaseCurveProfile {
  id: string;
  name: string;
  points: [number, number][];
  sampleCount: number;
}
export interface BaseCurveLibrary {
  cameraMake: string | null;
  cameraModel: string | null;
  profiles: BaseCurveProfile[];
}
export interface AutoAdjustResult {
  profile: BaseCurveProfile;
  exposure: number;
  contrast: number;
  saturation: number;
}
export async function getBaseCurveProfiles(repositoryId: string, assetId: number): Promise<BaseCurveLibrary | null> {
  if (!isTauriRuntime()) return null;
  return call<BaseCurveLibrary>("base_curve_profiles", { repositoryId, assetId });
}
export async function renameBaseCurveProfile(repositoryId: string, assetId: number, profileId: string, name: string): Promise<BaseCurveProfile | null> {
  if (!isTauriRuntime()) return null;
  return call<BaseCurveProfile>("base_curve_rename", { repositoryId, assetId, profileId, name });
}
export async function fitBaseCurveAndAutoAdjust(repositoryId: string, assetId: number): Promise<AutoAdjustResult | null> {
  if (!isTauriRuntime()) return null;
  return call<AutoAdjustResult>("base_curve_auto_adjust", { repositoryId, assetId });
}
