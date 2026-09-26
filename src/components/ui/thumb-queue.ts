/**
 * 缩略图加载队列（`specs/M1-5.md` §3.1）。
 *
 * 三件事，缺一个就会在滚动时出问题：
 *
 * 1. **限流**：一次最多渲染 4 张（渲染一张要几十到两百毫秒）。不限流的话，
 *    快速滚过一个目录会同时发起上百个请求，把 CPU 占满、界面反而更卡；
 * 2. **去重**：同一个文件被请求两次（滚出去又滚回来）只读一次；
 * 3. **回收**：`Blob` URL 不回收就是内存泄漏 —— 按 LRU 淘汰，淘汰时
 *    `URL.revokeObjectURL`。
 *
 * 缓存里存的是**已经转成 URL 的**结果（浏览器可以用它直接解码显示，
 * 不必把字节留在 JS 里）。`load` 与 URL 的创建/回收都是注入的 —— 测试里
 * 换成假的，就能把「限流 / 去重 / 淘汰」这三件事钉死，不需要真的浏览器。
 *
 * **换目录用 `clear()`**：它会丢弃整张表并回收 URL，同时推进一个「代号」——
 * 还在飞的请求回来时发现代号变了就直接丢掉，不会把上一个目录的缩略图
 * 补进新列表里（那种串图很难查）。
 *
 * **只改一张用 `refresh(path)`**：编辑落库后当前那张的缩略图要反映新编辑，
 * 但其它格子与它无关 —— `clear()` 会把整条胶片带每一格的 URL 都回收，
 * 松一次手全量重画（2026-09-24 人类报的「最严重」那一条）。
 */

import { createSignal, untrack } from "solid-js";

import { imageMimeOfBytes } from "../../lib/image-mime.ts";

export type ThumbStatus = "idle" | "loading" | "ready" | "error";

export interface ThumbEntry {
  status: ThumbStatus;
  /** `ready` 时的图片地址（`blob:` URL） */
  url: string | null;
}

/** 还没请求过的条目（视图据此显示占位） */
export const IDLE_THUMB: ThumbEntry = { status: "idle", url: null };

export interface ThumbQueueDeps {
  /** 取一个文件的缩略图字节；`null` 表示拿不到（浏览器里就是这样） */
  load: (path: string) => Promise<Uint8Array | null>;
  /** 字节 → 可显示的 URL（默认 `Blob` + `createObjectURL`） */
  toUrl?: (bytes: Uint8Array) => string;
  /** 回收 URL（默认 `URL.revokeObjectURL`） */
  revokeUrl?: (url: string) => void;
  /** 刷新替换前先解码新图，避免 URL 已切换而 WebView 尚未出图的闪白。 */
  prepareUrl?: (url: string) => Promise<void>;
  /** 同时在飞的请求数上限（默认 4） */
  concurrency?: number;
  /** 缓存条数上限（默认 600 —— 384px 的 JPEG 约 30–60KB，600 条 ≈ 20–35MB） */
  maxEntries?: number;
}

export interface ThumbQueue {
  /** 读一个条目的当前状态（不会触发请求） */
  get: (path: string) => ThumbEntry;
  /** 请求一张（已缓存或已在飞则忽略；失败过的允许重试） */
  request: (path: string, priority?: boolean) => void;
  /**
   * 让**一张**失效并立刻重取（编辑落库后这一张的缩略图要反映新编辑）。
   *
   * 与 `clear()` 的区别：只动这一条 —— 其它格子的 URL、节点身份与滚动位置都不受影响。
   * 在飞的旧请求按**按路径的代号**丢弃：旧结果回来时对不上就丢，不会把旧图盖在新图上。
   */
  refresh: (path: string) => void;
  /** 队内统计（排错与「加载中」提示用） */
  stats: () => { entries: number; inflight: number; queued: number };
  /** 丢弃全部缓存并回收 URL（换目录、卸载时调） */
  clear: () => void;
}

/**
 * 默认的字节 → URL。
 *
 * 这里**复制一份 buffer** 而不是直接把 `bytes` 塞给 `Blob`：`Uint8Array` 的
 * `buffer` 类型是 `ArrayBufferLike`（可能是 `SharedArrayBuffer`），
 * 在 TS 的新 lib 里与 `BlobPart` 不兼容，硬转会引入一个类型洞。
 * 缩略图只有几十 KB，复制一次无所谓。
 *
 * MIME **按字节自己的文件头判**（`lib/image-mime.ts`）—— 以前写死 `image/jpeg`，
 * 而管线现在吐 AVIF；写死任何一个都会在另一条路上撒谎。
 */
function defaultToUrl(bytes: Uint8Array): string {
  const buffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buffer).set(bytes);
  return URL.createObjectURL(new Blob([buffer], { type: imageMimeOfBytes(bytes) }));
}

function defaultRevoke(url: string): void {
  URL.revokeObjectURL(url);
}

async function defaultPrepareUrl(url: string): Promise<void> {
  if (typeof Image === "undefined") return;
  const image = new Image();
  image.src = url;
  await image.decode();
}

export function createThumbQueue(deps: ThumbQueueDeps): ThumbQueue {
  const concurrency = Math.max(1, Math.floor(deps.concurrency ?? 4));
  const maxEntries = Math.max(1, Math.floor(deps.maxEntries ?? 600));
  const toUrl = deps.toUrl ?? defaultToUrl;
  const revokeUrl = deps.revokeUrl ?? defaultRevoke;
  const prepareUrl = deps.prepareUrl ?? defaultPrepareUrl;

  const [entries, setEntries] = createSignal<Record<string, ThumbEntry>>({});
  const [inflight, setInflight] = createSignal(0);
  let queued: string[] = [];
  /** LRU 顺序：最近用过的在后面 */
  let recent: string[] = [];
  /** 代号：`clear()` 推进它，用来丢弃上一个目录还在飞的请求 */
  let generation = 0;
  /** 每条路径的代号：`refresh()` 推进它，用来丢弃这条路径上还在飞的旧请求 */
  const pathRevs = new Map<string, number>();
  const revOf = (path: string): number => pathRevs.get(path) ?? 0;

  const patch = (path: string, entry: ThumbEntry): void => {
    setEntries((prev) => ({ ...prev, [path]: entry }));
  };

  const touch = (path: string): void => {
    recent = recent.filter((item) => item !== path);
    recent.push(path);
  };

  /** 超出上限时淘汰最久没用到的**已完成**条目（在飞的不能淘汰） */
  const evict = (): void => {
    const all = entries();
    const total = Object.keys(all).length;
    if (total <= maxEntries) return;
    const evictable = recent.filter((path) => all[path]?.status !== "loading");
    const victims = evictable.slice(0, total - maxEntries);
    if (victims.length === 0) return;

    setEntries((prev) => {
      const next = { ...prev };
      for (const path of victims) {
        const entry = next[path];
        if (entry?.url) revokeUrl(entry.url);
        delete next[path];
      }
      return next;
    });
    recent = recent.filter((path) => !victims.includes(path));
  };

  const pump = (): void => {
    while (inflight() < concurrency && queued.length > 0) {
      const path = queued.shift() as string;
      const current = entries()[path];
      // 排队期间已经被清空（换目录）→ 跳过
      if (!current || current.status === "ready") continue;

      setInflight((n) => n + 1);
      patch(path, { status: "loading", url: current.url });

      const issuedAt = generation;
      const issuedRev = revOf(path);
      void deps
        .load(path)
        .then(async (bytes) => {
          // 上一个目录的 / 被 `refresh()` 作废的旧结果，丢掉
          if (issuedAt !== generation || issuedRev !== revOf(path)) return;
          if (bytes === null) {
            patch(path, { status: "error", url: entries()[path]?.url ?? null });
            return;
          }
          const url = toUrl(bytes);
          try {
            await prepareUrl(url);
          } catch {
            // 新图解码失败，旧图继续显示；新 URL 不能泄漏。
            revokeUrl(url);
            throw new Error("新缩略图解码失败"); // i18n-exempt: 队列内部诊断，界面不会展示
          }
          if (issuedAt !== generation || issuedRev !== revOf(path)) {
            revokeUrl(url);
            return;
          }
          const oldUrl = entries()[path]?.url;
          touch(path);
          patch(path, { status: "ready", url });
          if (oldUrl) revokeUrl(oldUrl);
          evict();
        })
        .catch(() => {
          if (issuedAt === generation && issuedRev === revOf(path)) {
            patch(path, { status: "error", url: entries()[path]?.url ?? null });
          }
        })
        .finally(() => {
          if (issuedAt !== generation) return;
          setInflight((n) => Math.max(0, n - 1));
          pump();
        });
    }
  };

  const request = (path: string, priority = false): void => {
    const queuedAt = queued.indexOf(path);
    if (queuedAt >= 0) {
      if (priority && queuedAt > 0) {
        queued.splice(queuedAt, 1);
        queued.unshift(path);
      }
      return;
    }
    const current = entries()[path];
    if (current && current.status !== "error") return; // 已在飞 / 已完成
    // 失败过的允许重试（文件被占用这类问题常常是暂时的）
    patch(path, { status: "loading", url: null });
    if (priority) queued.unshift(path);
    else queued.push(path);
    pump();
  };

  return {
    get: (path) => entries()[path] ?? IDLE_THUMB,

    // Request writes queue signals; callers track their path, never queue internals.
    // Otherwise an error publication immediately retries forever from the same effect.
    request: (path, priority) => untrack(() => request(path, priority)),

    stats: () => ({
      entries: Object.keys(entries()).length,
      inflight: inflight(),
      queued: queued.length,
    }),

    refresh: (path) => {
      // 在飞的旧请求作废，但旧 URL 一直留在视图中。新图解码后再原子换 URL。
      // `untrack` 同 `clear()`：`refresh` 通常也在 effect 里被调，读 `entries` 不能进依赖。
      pathRevs.set(path, revOf(path) + 1);
      const current = untrack(entries)[path];
      queued = queued.filter((item) => item !== path);
      patch(path, { status: "loading", url: current?.url ?? null });
      queued.push(path);
      untrack(pump);
    },

    clear: () => {
      generation += 1;
      // 这一次读**必须** untrack：`clear()` 通常是在 effect 里被调的（换库 / 换目录时清缓存）。
      // 若把 `entries()` 读进依赖，紧随其后的 `setEntries({})` 就会让**调用它的那个 effect 自我失效** →
      // 重跑 → 再读再写（每次都是新对象，Solid 的等值判断短路不掉）→ 无限自激直到爆栈。
      // 实测（2026-09-17）：人类机器上「库在线时进浏览」必爆 `Maximum call stack size exceeded`，
      // 而浏览器冒烟抓不到 —— 那里没有库，`root` 恒为 null，调用方的 effect 提前 return 了。
      for (const entry of Object.values(untrack(entries))) {
        if (entry.url) revokeUrl(entry.url);
      }
      queued = [];
      recent = [];
      setEntries({});
      setInflight(0);
    },
  };
}
