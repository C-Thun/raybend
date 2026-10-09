/**
 * 看图取图的**探针**：记录照片 URL 的一生（谁借走、谁放手、有没有被回收），
 * 给「对比里点一张、另一张变空白」这类**界面上没有报错、只有眼睛能看出症状**的问题用。
 *
 * ## 为什么要有它（2026-10-08 的实例）
 *
 * 对比视图每一格各要一份 URL（`store.imageUrlFor`）。一个 URL 的一生会跨三处持有者
 * （单图槽位 / 总览槽位 / 多图缓存），而且全程异步（取图、迟到作废、LRU 淘汰）。
 * 出问题时的症状是「某一格整块空白」—— DOM 上没有任何提示，控制台也没有报错，
 * 只能靠人在真机上描述。探针把这段生命周期变成**可读的事件流 + 每张照片还活着几个 URL**，
 * 于是在运行中的窗口里就能一眼分开两种完全不同的病：
 *
 * * 空白而 `live = 0` ⇒ URL 被提前回收（存储层所有权问题，2026-10-08 那次就是它）；
 * * 空白而 `live > 0` ⇒ URL 还活着，是视图/布局侧没用上（该往渲染侧查）。
 *
 * 这也是**唯一**的判据分裂点：没有它，「一格空白」只能靠逐行读代码猜。
 *
 * ## 怎么开
 *
 * | 方式 | 适用 |
 * | --- | --- |
 * | `localStorage["raybend.viewer-probe.v1"] = "1"` 后刷新 | 开发版与打包版都能用（打包版只装这个开关，不装热键） |
 * | `Ctrl+Alt+Shift+D` | 开发版（`import.meta.env.DEV`）随时开关，不用刷新 |
 * | 控制台 `__raybendProbe.enable()` / `.dump()` / `.events()` | 开发者句柄（运行中的窗口直接可用） |
 *
 * 关闭时 `record()` 第一件事就是返回 —— 生产构建里它只是几个布尔判断：
 * 没有监听器、没有定时器、不建事件对象。
 *
 * ## 与「日志」的分工
 *
 * 它**不是**通用日志：只记 URL 生命周期这一条线，容量固定（环形 160 条），
 * 不占磁盘、不上报。要看现场的人用它；要留证据的写 `implementations/`。
 */

import { createSignal } from "solid-js";

/** URL 的一生里会被记下来的动作 */
export type ViewerProbeKind =
  /** 多图路径开始为某张取图 */
  | "ensure"
  /** 字节到了、造出 URL */
  | "new"
  /** 单图路径借用了缓存里那份（不摘走） */
  | "borrow"
  /** 某处放手：`detail` 说明是否真的回收、谁还持有 */
  | "release"
  /** 真正 `URL.revokeObjectURL` */
  | "revoke"
  /** 被挤出多图缓存 */
  | "evict"
  /** 磁盘源变化 / 换目录 / 关闭导致失效 */
  | "invalidate"
  /** 视图要一张图，但它没有任何 URL —— **这一条就是「空白那一格」** */
  | "missing";

export interface ViewerProbeEvent {
  seq: number;
  /** `performance.now()` 取整（毫秒）；没有性能时钟的环境是 0 */
  at: number;
  kind: ViewerProbeKind;
  /** 照片 key（`id\0path`）或 URL 反查到的 key；空串 = 与具体照片无关 */
  key: string;
  url: string | null;
  detail: string;
}

/** 事件环绕缓冲的容量：够看到一次交互的前后文，又不会吃内存 */
const CAPACITY = 160;

/** 探针开关的存储键（设备级；与 `lib/*-prefs.ts` 同一命名法） */
export const VIEWER_PROBE_STORAGE_KEY = "raybend.viewer-probe.v1";

/** `Ctrl+Alt+Shift+D`：与命令注册表里的键位无冲突（那条线是产品功能，这条线只属于开发者） */
const TOGGLE_KEY = "d";

export interface ViewerProbeDeps {
  /** 注入存储（测试用；缺省读 `localStorage`，读不到就当作未开） */
  storage?: { getItem: (key: string) => string | null } | undefined;
  /** 直接指定初始开关（测试用；给了它就不看存储） */
  enabled?: boolean;
  /** 是否安装键盘开关（缺省：`import.meta.env.DEV === true` 或存储里显式开了） */
  keyToggle?: boolean;
}

export interface ViewerUrlProbe {
  enabled: () => boolean;
  /** 开关一次；返回开还是关（热键用） */
  toggle: () => boolean;
  setEnabled: (on: boolean) => void;
  /*
   * 版本号：探针开着时**每记一条跳一下**，浮层跟着事件流刷新。
   * 关着时 `record` 提前返回，不会跳 —— 也不会因此拖慢渲染。
   */
  version: () => number;
  /** 记一条事件（关着时直接返回）；省略的字段按空值处理 */
  record: (event: {
    kind: ViewerProbeKind;
    key?: string;
    url?: string | null;
    detail?: string;
  }) => void;
  events: () => readonly ViewerProbeEvent[];
  /** 这张照片当前还活着几个 URL（建了还没回收） */
  liveFor: (key: string) => number;
  liveTotal: () => number;
  /** 某个 URL 当初是哪张照片的（记不到就返回空串） */
  keyOf: (url: string) => string;
  clear: () => void;
  /** 人读的现场快照（控制台 `__raybendProbe.dump()`） */
  dump: () => string;
}

/**
 * 有没有浏览器环境（Node 单测里没有 `document`）。
 *
 * 必须有这道闸：Node 26 的 `globalThis.localStorage` 是个会打 `ExperimentalWarning`
 * 的 getter，单测里白读一下就会往输出里喷一条警告。
 */
function hasDom(): boolean {
  return globalThis.document !== undefined;
}

function readFlag(storage: ViewerProbeDeps["storage"]): boolean {
  const source =
    storage ??
    (hasDom()
      ? (() => {
          try {
            return globalThis.localStorage;
          } catch {
            // 隐私模式 / 禁用存储：当作没开
            return undefined;
          }
        })()
      : undefined);
  try {
    return source?.getItem(VIEWER_PROBE_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function now(): number {
  return Math.round(globalThis.performance?.now?.() ?? 0);
}

/**
 * 造一个探针（单例见文件尾；测试用工厂，各自独立互不干扰）。
 */
export function createViewerUrlProbe(deps: ViewerProbeDeps = {}): ViewerUrlProbe {
  const initial = deps.enabled ?? readFlag(deps.storage);
  const [enabled, setEnabledSignal] = createSignal(initial);
  const [version, setVersion] = createSignal(0);
  /** 环形缓冲：只留最近 CAPACITY 条 */
  let events: ViewerProbeEvent[] = [];
  let seq = 0;
  /** 活着的 URL：key → 该照片的 URL 集合（一个 URL 只属于一张照片） */
  const live = new Map<string, Set<string>>();
  const urlOwner = new Map<string, string>();

  const bump = (): void => {
    setVersion((value) => value + 1);
  };
  function addLive(key: string, url: string): void {
    if (key === "") return;
    const set = live.get(key) ?? new Set<string>();
    set.add(url);
    live.set(key, set);
    urlOwner.set(url, key);
  }

  function dropLive(url: string): void {
    const key = urlOwner.get(url);
    if (key === undefined) return;
    urlOwner.delete(url);
    const set = live.get(key);
    if (set === undefined) return;
    set.delete(url);
    if (set.size === 0) live.delete(key);
  }

  function setEnabled(on: boolean): void {
    if (on === enabled()) return;
    setEnabledSignal(on);
    bump();
  }

  function record(event: {
    kind: ViewerProbeKind;
    key?: string;
    url?: string | null;
    detail?: string;
  }): void {
    if (!enabled()) return;
    const key = event.key ?? "";
    const url = event.url ?? null;
    /*
     * 同 key 同 kind 的连续重复只记第一条：`imageUrlFor` 在渲染期会被问很多次，
     * 「这一格还是没 URL」连记一百条只会把真正的前因冲掉。
     */
    const last = events.length === 0 ? undefined : events[events.length - 1];
    if (last !== undefined && last.kind === event.kind && last.key === key && last.url === url) return;

    if (event.kind === "new" && url !== null) addLive(key, url);
    if (event.kind === "revoke" && url !== null) dropLive(url);

    seq += 1;
    const entry: ViewerProbeEvent = {
      seq,
      at: now(),
      kind: event.kind,
      key,
      url,
      detail: event.detail ?? "",
    };
    events = events.length >= CAPACITY ? [...events.slice(events.length - CAPACITY + 1), entry] : [...events, entry];
    bump();
  }

  function toggle(): boolean {
    setEnabled(!enabled());
    return enabled();
  }

  function dump(): string {
    const lines = [
      `viewer probe: ${enabled() ? "ON" : "off"} · live urls ${liveTotal()} · events ${events.length}/${CAPACITY}`,
      ...events.slice(-40).map((event) => {
        const url = event.url === null ? "" : ` ${event.url}`;
        const detail = event.detail === "" ? "" : ` · ${event.detail}`;
        return `#${event.seq} ${event.at}ms ${event.kind} ${event.key}${url}${detail}`;
      }),
    ];
    return lines.join("\n");
  }

  function liveTotal(): number {
    let total = 0;
    for (const set of live.values()) total += set.size;
    return total;
  }

  if (deps.keyToggle === true && hasDom()) {
    globalThis.addEventListener("keydown", (event) => {
      const chord = event as KeyboardEvent;
      if (chord.ctrlKey && chord.altKey && chord.shiftKey && chord.key.toLowerCase() === TOGGLE_KEY) {
        chord.preventDefault();
        toggle();
      }
    });
  }

  return {
    enabled,
    toggle,
    setEnabled,
    version,
    record,
    events: () => events,
    liveFor: (key) => live.get(key)?.size ?? 0,
    liveTotal,
    keyOf: (url) => urlOwner.get(url) ?? "",
    clear: () => {
      events = [];
      seq = 0;
      bump();
    },
    dump,
  };
}

/** 是否开发期构建（Node 单测里 `import.meta.env` 是 undefined） */
const isDev = import.meta.env?.DEV === true;

/** 模块级单例：store 与视图共用它，页面只有一个事件流 */
export const viewerUrlProbe = createViewerUrlProbe({
  keyToggle: isDev || readFlag(undefined),
});

/** 开发者句柄（控制台直接敲 `__raybendProbe.dump()`） */
if (hasDom()) {
  (globalThis as { __raybendProbe?: ViewerUrlProbe }).__raybendProbe = viewerUrlProbe;
}
