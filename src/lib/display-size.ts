/**
 * **显示变体的唯一前端读口**（`specs/display-size-source.md` §6.1）。
 *
 * 两件事：
 *
 * 1. **显示尺寸**：「这张照片现在显示的到底是多大」只有一个答案 —— 库里存好的那个
 *    变体尺寸；库里没存（老库还没回填）时后端读侧已按当前栈/定稿**现算**过
 *    （零 I/O，`store::query::page` / `store::issues::list`），所以这里拿到的
 *    不会退回**基准画面**（那是另一个口径，裁切过的比例可以完全不同）。
 * 2. **来源基底**（`sourceBaseOf`）：flowinfo 的「文件类型」随显示变体走
 *    （崔总 2026-10-11 定）—— SOOC/RAW 是它们自己，latest 与定稿看各自拿谁渲染的。
 *
 * 本文件只做**选择与回退**——方向、裁切、旋转的数学全在 Rust
 * （`store::sizes` / `media::meta::oriented_size` / `EditGeometry::output_size`），
 * 前端不算第二份（`AGENTS.md` §2.12）。
 *
 * 变体与尺寸的对应（与 Rust 侧 `store::sizes::display_size` 的矩阵逐条一致）：
 *
 * | 显示变体 | 用哪个字段 | 缺失回退 |
 * | --- | --- | --- |
 * | 没选（默认） / `latest` | `AssetItem.latestWidth/Height` | `width/height` |
 * | `sooc` | `AssetItem.width/height`（基准文件的展示尺寸） | —— |
 * | `raw` | `AssetItem.rawWidth/Height` | `width/height` |
 * | `issue:N` | 该定稿的 `Issue.width/height` | `width/height` |
 *
 * 全部是**库里已有的值**：不读文件、不渲染、不等任何后台任务。
 */

/*
 * 入参用**结构形状**（不 import `api/` 的类型）：`lib` 层只允许向下依赖 `assets`
 * （`scripts/check-architecture.mjs` 的 LAYERS）。`AssetItem` / `Issue` 字段更多，
 * 自然满足这些形状 —— 这里声明的是函数真正用到的最小契约。
 */

/** 尺寸解析需要的列表项字段（`AssetItem` 满足它）。 */
export interface DisplaySizeAssetRow {
  /** 基准文件的展示尺寸（位图优先）。 */
  width: number | null;
  height: number | null;
  /** RAW 文件的展示尺寸。 */
  rawWidth: number | null;
  rawHeight: number | null;
  /** latest 的输出尺寸。 */
  latestWidth: number | null;
  latestHeight: number | null;
  /** latest 的来源基底（`"raw"` / `"sooc"`；无编辑栈 → `null`）。 */
  latestSourceBase: string | null;
}

/** 尺寸解析需要的定稿字段（`Issue` 满足它）。 */
export interface DisplaySizeIssueRow {
  id: number;
  width: number | null;
  height: number | null;
  /** 该定稿的渲染来源基底（`"raw"` / `"sooc"`）。 */
  sourceBase: string;
}

/** 定稿列表的形状（`IssueLibrary` 满足它）。 */
export interface DisplaySizeIssueList {
  readonly issues: readonly DisplaySizeIssueRow[];
}

export interface DisplaySize {
  width: number;
  height: number;
}

/** 两列都在且为正才算「已知」；坏值当未知（回退，不显示 0×0）。 */
function sized(width: number | null, height: number | null): DisplaySize | null {
  return width !== null && height !== null && width > 0 && height > 0
    ? { width, height }
    : null;
}

/** 基准展示尺寸：`width/height`（Rust 侧已是展示尺寸；位图优先，没有位图就是 RAW）。 */
export function baseDisplaySize(asset: DisplaySizeAssetRow): DisplaySize | null {
  return sized(asset.width, asset.height);
}

/** `issue:7` → `7`；认不出（`issue:` 空、`issue:0`、其它字符串）给 `null`，不猜。 */
export function issueIdOf(choice: string): number | null {
  if (!choice.startsWith("issue:")) return null;
  const id = Number(choice.slice("issue:".length));
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

/** 从定稿列表里取 `issue:N` 对应的那条（没有列表 / 找不到 → `undefined`）。 */
export function issueOf(
  library: DisplaySizeIssueList | null,
  choice: string | undefined,
): DisplaySizeIssueRow | undefined {
  if (library === null || choice === undefined) return undefined;
  const id = issueIdOf(choice);
  if (id === null) return undefined;
  return library.issues.find((issue) => issue.id === id);
}

/**
 * 当前显示变体的尺寸（`null` = **还不知道**，调用方就按「未知」处理 —— 绝不拿别的变体凑）。
 *
 * 三条硬规矩（崔总 2026-10-11 真机：「view 里 raw 图尺寸没有跟着本身走，
 * 而是缩进了 sooc 的尺寸里」—— 就是下面那些 `?? base` 干的）：
 *
 * 1. **每个变体只认自己那一份**：`raw` ← `rawWidth/Height`；`issue:N` ← 该定稿的行；
 *    `sooc` = 基准；`latest`（含没选）← `latestWidth/Height`。
 * 2. **缺值 ≠ 退回别的变体**。老库没回填时 RAW 行是 NULL —— 那是「还没拿到」，
 *    返回 `null`，由调用方去补（`export_snapshots` 会顺手读一次文件头写回）
 *    或者走「尺寸未知」的状态；拿基准凑会让同一张图在**两次访问之间换倍率**
 *    （懒回填跑前 / 跑后），比慢一点糟得多。
 * 3. 唯一允许的 `?? base` 是 `latest`（含没选）那一支：**没编辑栈时 latest 的定义
 *    就是基准尺寸**（Rust `store::sizes::latest_size` 同一条）—— 同一个变体，不是别人。
 */
export function displaySizeOf(
  asset: DisplaySizeAssetRow,
  choice: string | undefined,
  issue: DisplaySizeIssueRow | undefined,
): DisplaySize | null {
  const base = baseDisplaySize(asset);
  if (choice === undefined || choice === "latest") {
    return sized(asset.latestWidth, asset.latestHeight) ?? base;
  }
  if (choice === "sooc") return base;
  if (choice === "raw") return sized(asset.rawWidth, asset.rawHeight);
  const id = issueIdOf(choice);
  if (id === null || issue === undefined || issue.id !== id) return null;
  return sized(issue.width, issue.height);
}

/**
 * 当前显示变体的**来源基底**：`"raw"` / `"sooc"` / `null`（说不清）。
 *
 * 用途是 flowinfo 的「文件类型」（`features/exif-strip` 把它翻译成 RAW / JPEG…）：
 * RAW 基底显示 `RAW`，SOOC 基底显示基准位图自己的格式 ——
 * 与变体下拉里「定稿名 · RAW/SOOC」的叫法同源（崔总 2026-10-11 定）。
 *
 * * `sooc` / `raw` 变体：它们自己就是基底；
 * * `latest`（含没选）：编辑栈的 `source_base`（没编辑栈 → `null`）；
 * * `issue:N`：该定稿的 `source_base`（找不到那条 → `null`）。
 */
export function sourceBaseOf(
  asset: DisplaySizeAssetRow,
  choice: string | undefined,
  issue: DisplaySizeIssueRow | undefined,
): "raw" | "sooc" | null {
  if (choice === "sooc") return "sooc";
  if (choice === "raw") return "raw";
  if (choice === undefined || choice === "latest") return baseValue(asset.latestSourceBase);
  const id = issueIdOf(choice);
  if (id === null || issue === undefined || issue.id !== id) return null;
  return baseValue(issue.sourceBase);
}

function baseValue(value: string | null | undefined): "raw" | "sooc" | null {
  return value === "raw" || value === "sooc" ? value : null;
}
