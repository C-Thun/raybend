/**
 * 编辑视口的 IPC 封装（`src/` 里只有本目录可以直接 `invoke`，见 `ARCHITECTURE.md` §1.1）。
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
} from "./types.ts";
import type { EditorViewportPayload } from "../lib/editor-viewport.ts";
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
): Promise<EditorRenderState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorRenderState>("editor_viewport_intent", { intent });
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
export interface DevelopStack {
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
   * 镜头配置文件（`null` = 自动识别；`"none"` = 显式关掉自动匹配；否则是 `maker|model`）。
   *
   * 为什么 `null` 与 `"none"` 不同：自动识别是**会变的**（换了 EXIF 读法、或库里新增了
   * 这支镜头）—— 「我不要自动匹配」这件事必须能存住。
   */
  lensProfile?: string | null;
  /** 配置文件那一半的开关（`null` = 默认开）。**手动三根拉杆不受它影响**。 */
  lensEnabled?: boolean | null;
  /** 降噪方式（`null` = 快速档；`"high"` = BM3D 高质量档）。 */
  nrMethod?: string | null;
}

/** 降噪方式（编辑栈的一级；`"high"` = BM3D 高质量档，后台任务）。 */
export type DevelopNrMethod = "fast" | "high";

/**
 * 镜头配置文件（lensfun 库里的一支镜头）。
 *
 * `key` 是稳定键（`maker|model`）—— 存进编辑栈的是它，不是序号。
 */
export interface LensProfile {
  key: string;
  maker: string;
  model: string;
  /** 投影类型是矩形吗（鱼眼 / 全景的**几何**校正本轮不做，界面要写明） */
  rectilinear: boolean;
}

/** 这张照片的镜头匹配状态（自动识别 + 下拉候选）。 */
export interface LensMatch {
  /** 库里就绪了吗（`false` ⇒ 界面显示「加载中」，不是「没匹配到」） */
  ready: boolean;
  /** 自动识别到的配置文件（没有就是 `null` —— **不猜**） */
  detected: LensProfile | null;
  /** EXIF 里的镜头字符串（拿它解释「为什么没匹配到」） */
  lensName: string | null;
  /** 下拉候选（自动匹配的排第一；库里没有就空） */
  candidates: LensProfile[];
}

/** 编辑栈里**不是参数也不是曲线**的那几项（落库与回读都用这个形状）。 */
export interface DevelopSettings {
  lensProfile?: string | null;
  lensEnabled?: boolean | null;
  nrMethod?: DevelopNrMethod | null;
}

/** 这张照片的镜头匹配状态 + 候选（进编辑时问一次；库没就绪时 `ready = false`）。 */
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
    values: stack.values,
    curves: stack.curves,
    asShotK: stack.asShotK ?? null,
    lensProfile: stack.lensProfile ?? null,
    lensEnabled: stack.lensEnabled ?? null,
    nrMethod: stack.nrMethod ?? null,
  };
  return call<DevelopCommitResult>("develop_commit", {
    repositoryId,
    assetId,
    stack: payload,
  });
}

/** 重置全部（清掉这张照片的编辑栈）。 */
export async function resetDevelopStack(
  repositoryId: string,
  assetId: number,
): Promise<DevelopCommitResult | null> {
  if (!isTauriRuntime()) return null;
  return call<DevelopCommitResult>("develop_reset", { repositoryId, assetId });
}

/**
 * **刷新 preview**（`IMAGING.md` §4）：编辑器**进 / 出**两个节点各调一次。
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
 * **编辑器该编辑哪个文件**（「编辑落在 RAW 上」，`REPOSITORY.md` §4.1）。
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
