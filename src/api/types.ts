/**
 * Rust 侧 IPC 结构的 **TS 镜像**（`plans/M1-5.md` §3.1 的「手写镜像」决定）。
 *
 * 约定（**改这里必须同时改 Rust 侧**）：
 *   - 字段名与 Rust 的 `#[serde(rename_all = "camelCase")]` 输出**逐字一致**；
 *   - Rust 的 `Option<T>` 序列化成 `null`（**不是**缺席的键），所以这里写 `| null`
 *     而不是 `?:`——「字段不存在」和「字段是空」在界面上是两件事；
 *   - 这些类型只描述**传输形状**。界面上要用的形状（如 `ExifData` 的 `exposureSeconds`）
 *     由各自的 feature 负责转换，避免把「怎么显示」塞进传输层。
 *
 * 防漂移：`src/api/dto-contract.json` 是两侧共用的键名清单 ——
 * `dto-contract.test.ts`（TS）与 `src-tauri/src/contract.rs`（Rust）都对着它断言，
 * 谁改了字段名而没同步，就有一侧先红。
 */

/** 来源类型（`raybend::store::volumes::VolumeKind::code`） */
export type VolumeKind =
  | "local"
  | "removable"
  | "optical"
  | "network"
  | "cloud"
  | "unknown";

/** 媒体大类（`raybend::media::kind::MediaKind`） */
export type MediaKind = "raw" | "image" | "other";

/** 拍摄时间的来源（`raybend::media::exif::TakenAtSource`） */
export type TakenAtSource = "exif" | "filename" | "file_mtime";

/** 缩略图尺度（`raybend::thumbnail::render::SizeClass`） */
export type ThumbSize = "grid" | "strip";

/** 最近导入过的一个目录（`store::recent::RecentDir`）。 */
export interface RecentDir {
  path: string;
  includeSubdirs: boolean;
  usedAt: number;
  useCount: number;
}

/** 一个可选来源（驱动器 / 挂载点）。 */
export interface Volume {
  path: string;
  kind: VolumeKind;
  /** 中文名（提示文案用；界面文案仍走 i18n，这里只用于排错与 tooltip 兜底）。 */
  kindLabel: string;
}

/** 目录树里的一层子目录。 */
export interface DirEntry {
  name: string;
  path: string;
}

/** 中列里的一张照片。 */
export interface SourceItem {
  path: string;
  fileName: string;
  /** 小写扩展名（不含点）；无扩展名时 `null`。 */
  ext: string | null;
  kind: MediaKind;
  sizeBytes: number;
  /** 文件修改时间（Unix 毫秒）。 */
  mtimeMs: number | null;
  /** **快速兜底**的拍摄时间：来自文件名或 mtime；真相要用 `readSourceTimes` 补。 */
  takenAtMs: number | null;
  takenAtSource: TakenAtSource | null;
  /**
   * 拍摄时间的时区偏移（分钟，东八区 = 480）。
   * `null` = 相机没写时区，`takenAtMs` 是「墙上时间当 UTC」（M1-3 的口径）——
   * 显示与分组都要按 **UTC** 处理，才对得上相机里的数字。
   */
  takenAtOffsetMin: number | null;
}

/** 一次目录列取的结果。 */
export interface SourceScan {
  root: string;
  items: SourceItem[];
  /** 跳过的文件/目录数（隐藏、垃圾、侧车、非照片）。 */
  skipped: number;
  /** 读不了的位置（权限等）——不是致命错误。 */
  problems: string[];
  elapsedMs: number;
}

/** 一个文件的精确拍摄时间。 */
export interface TimeEntry {
  path: string;
  takenAtMs: number | null;
  takenAtSource: TakenAtSource | null;
  takenAtOffsetMin: number | null;
}

/** 照片计数（喂「已选择 N 张照片」）。 */
export interface PhotoCount {
  photos: number;
  skipped: number;
  /** 扫描是否被中止（为真时 `photos` 是**下界**）。 */
  truncated: boolean;
}

/** 一个文件的 EXIF（喂 `flowbar` 的图片信息区）。 */
export interface FileExif {
  cameraMake: string | null;
  cameraModel: string | null;
  lens: string | null;
  focalMm: number | null;
  fNumber: number | null;
  /** 快门时间（**毫秒**；界面自己换算成 `1/125s` 这种写法）。 */
  exposureMs: number | null;
  iso: number | null;
  width: number | null;
  height: number | null;
  orientation: number | null;
  takenAtMs: number | null;
  takenAtSource: TakenAtSource | null;
  takenAtOffsetMin: number | null;
  ext: string | null;
  kind: MediaKind;
}

/** 库的一条登记路径。 */
export interface RepositoryPath {
  path: string;
  /** `online` / `offline` / `unknown`。 */
  status: string;
  lastSeenAt: number | null;
}

/** 一个库（右列的库卡片）。 */
export interface RepositoryView {
  id: string;
  name: string;
  importTemplate: string | null;
  createdAt: number;
  lastOpenedAt: number | null;
  online: boolean;
  /** 在线时的库根目录。 */
  root: string | null;
  /** 卡片上显示哪条路径：在线 = 库根，离线 = 上次已知路径。 */
  displayPath: string;
  paths: RepositoryPath[];
  /**
   * 库里的照片数。
   * **`null` 不等于 0**：离线或读不到时是 `null`，界面显示「—」。
   */
  photoCount: number | null;
  /** 探测过几条路径（离线时给「已试过 N 处」的提示）。 */
  triedPaths: number;
}

/** 建库探测的结果种类。 */
export type RootProbeKind = "notDirectory" | "empty" | "existing" | "broken";

/** 建库弹窗在按下确认前看到的东西。 */
export interface RepositoryProbe {
  kind: RootProbeKind;
  /** 目录里已有库时的库名。 */
  name: string | null;
  repositoryId: string | null;
  /** 这个 ID 是否**已经登记过** —— 是的话这次操作是「给已有的库加一条路径」。 */
  registered: boolean;
  /** `broken` 时的原因（给用户看）。 */
  message: string | null;
}

/** `thumb_sources_stats` 的返回（缓存统计）。 */
export interface ThumbCacheStats {
  entries: number;
  bytes: number;
  pinned: number;
  bySize: Array<[string, number, number]>;
}

/* ══════════════════════════════════════════════════════════════
 * 导入执行与进度（M1-6）
 * ══════════════════════════════════════════════════════════════ */

/**
 * 导入阶段。**扫描与规划没有总数**，所以那两段的百分比是 `null`
 * （界面用不确定进度条，而不是编一个数字）。
 */
export type ImportStage = "scan" | "plan" | "import" | "thumbs" | "done";

/** 整批状态。`cancelled` / `done` / `failed` 是终态。 */
export type ImportState =
  | "running"
  | "pausing"
  | "paused"
  | "cancelling"
  | "cancelled"
  | "done"
  | "failed";

/** 正在处理的文件（「它没卡住」这件事靠它证明）。 */
export interface ImportCurrentItem {
  source: string;
  target: string | null;
}

/** 一条错误（失败；跳过不算错误，另有计数）。 */
export interface ImportError {
  source: string;
  target: string | null;
  reason: string;
  /** `failed` / `skipped`。 */
  status: string;
}

/** 一个源目录（一个 run）的进度。 */
export interface ImportRunProgress {
  runId: number;
  sourceRoot: string;
  stage: ImportStage;
  state: ImportState;
  /** 扫描/读元数据阶段就在涨。 */
  scanned: number;
  /** 要处理的条目总数（规划完才知道；之前是 0）。 */
  total: number;
  done: number;
  imported: number;
  skipped: number;
  /** 跳过里有多少是「已在库中」。 */
  duplicates: number;
  failed: number;
  bytes: number;
  current: ImportCurrentItem | null;
  note: string | null;
}

/** 整批快照（`import://progress` 事件的载荷、`import_status` 的返回）。 */
export interface ImportBatchProgress {
  batchId: string;
  stage: ImportStage;
  state: ImportState;
  /** 每个源目录一条。 */
  runs: ImportRunProgress[];
  /** 正在跑第几个（`[2/3]` 那种提示用它）。 */
  currentRun: number | null;
  total: number;
  done: number;
  imported: number;
  skipped: number;
  duplicates: number;
  failed: number;
  bytes: number;
  /** 错误清单（只留尾巴，最多 200 条）。 */
  errors: ImportError[];
  /** 错误总数（清单被截断了也知道有多少）。 */
  errorsTotal: number;
  freeBytes: number | null;
  startedAt: number;
  finishedAt: number | null;
}

/** 开工前的预检结果。 */
export interface ImportPrecheck {
  totalBytes: number;
  freeBytes: number | null;
  /** 目标卷空间偏紧（界面该问一句再开工）。 */
  tight: boolean;
  /** 判断里用的「需要多少」（含 5% 余量）。 */
  neededBytes: number;
}

/** 这批要跑哪些源目录（`runId` 在该目录真的开始时才分配）。 */
export interface ImportPlannedRun {
  index: number;
  sourceRoot: string;
}

/** `import_start` 的结果。 */
export interface ImportStart {
  batchId: string;
  runs: ImportPlannedRun[];
}

/** 上次被中断的导入（应用重启后提示「可继续」）。 */
export interface InterruptedRun {
  runId: number;
  sourceRoot: string;
  template: string;
  startedAt: number;
  imported: number;
  skipped: number;
  failed: number;
}
