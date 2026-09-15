/**
 * 缩略图加载队列（`plans/M1-5.md` §3.1）。
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
 */

import { createSignal } from "solid-js";

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
  /** 同时在飞的请求数上限（默认 4） */
  concurrency?: number;
  /** 缓存条数上限（默认 600 —— 384px 的 JPEG 约 30–60KB，600 条 ≈ 20–35MB） */
  maxEntries?: number;
}

export interface ThumbQueue {
  /** 读一个条目的当前状态（不会触发请求） */
  get: (path: string) => ThumbEntry;
  /** 请求一张（已缓存或已在飞则忽略；失败过的允许重试） */
  request: (path: string) => void;
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
 */
function defaultToUrl(bytes: Uint8Array): string {
  const buffer = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(buffer).set(bytes);
  return URL.createObjectURL(new Blob([buffer], { type: "image/jpeg" }));
}

function defaultRevoke(url: string): void {
  URL.revokeObjectURL(url);
}

export function createThumbQueue(deps: ThumbQueueDeps): ThumbQueue {
  const concurrency = Math.max(1, Math.floor(deps.concurrency ?? 4));
  const maxEntries = Math.max(1, Math.floor(deps.maxEntries ?? 600));
  const toUrl = deps.toUrl ?? defaultToUrl;
  const revokeUrl = deps.revokeUrl ?? defaultRevoke;

  const [entries, setEntries] = createSignal<Record<string, ThumbEntry>>({});
  const [inflight, setInflight] = createSignal(0);
  let queued: string[] = [];
  /** LRU 顺序：最近用过的在后面 */
  let recent: string[] = [];
  /** 代号：`clear()` 推进它，用来丢弃上一个目录还在飞的请求 */
  let generation = 0;

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
    const evictable = recent.filter((path) => all[path]?.status === "ready");
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
      patch(path, { status: "loading", url: null });

      const issuedAt = generation;
      void deps
        .load(path)
        .then((bytes) => {
          if (issuedAt !== generation) return; // 上一个目录的结果，丢掉
          if (bytes === null) {
            patch(path, { status: "error", url: null });
            return;
          }
          const url = toUrl(bytes);
          touch(path);
          patch(path, { status: "ready", url });
          evict();
        })
        .catch(() => {
          if (issuedAt === generation) patch(path, { status: "error", url: null });
        })
        .finally(() => {
          if (issuedAt !== generation) return;
          setInflight((n) => Math.max(0, n - 1));
          pump();
        });
    }
  };

  return {
    get: (path) => entries()[path] ?? IDLE_THUMB,

    request: (path) => {
      const current = entries()[path];
      if (current && current.status !== "error") return; // 已在飞 / 已完成
      if (queued.includes(path)) return;
      // 失败过的允许重试（文件被占用这类问题常常是暂时的）
      patch(path, { status: "loading", url: null });
      queued.push(path);
      pump();
    },

    stats: () => ({
      entries: Object.keys(entries()).length,
      inflight: inflight(),
      queued: queued.length,
    }),

    clear: () => {
      generation += 1;
      for (const entry of Object.values(entries())) {
        if (entry.url) revokeUrl(entry.url);
      }
      queued = [];
      recent = [];
      setEntries({});
      setInflight(0);
    },
  };
}
