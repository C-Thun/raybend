/**
 * 浏览工作区的左右两列（`BROWSE.md` §4 / §6、`design/browse.md` §2.2 / §2.4）。
 *
 * 左列 = **库目录选择器**：搜索条 + 紧缩库列表 + 库内目录树。
 * 右列 = **信息栏**：tiles 模式下显示 EXIF 与文件信息（看图模式另有预览+直方图，属 W2）。
 *
 * 两列放在同一个文件里，是因为它们共享同一份「当前库 / 当前范围」的读法，
 * 而各自的逻辑都很薄（都是「把状态摆出来 + 把点击转成回调」）。
 */

import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

import { listDirs } from "../../api/db.ts";
import type { AssetItem, RepositoryView } from "../../api/types.ts";
import { t } from "../../i18n/index.ts";
import { shortPath } from "../../lib/shortpath.ts";
import type { BrowseStore } from "./store.ts";

/* ══════════════════════════════════════════════════════════════
 * 左列：库目录选择器
 * ══════════════════════════════════════════════════════════════ */

/** 紧缩库列表最多显示几个（第 4 个只露半截，见 `BROWSE.md` §4.2）。 */
export const COMPACT_REPO_LIMIT = 3;

export interface BrowseLeftColumnProps {
  store: BrowseStore;
  /** 库列表（由工作区提供，因为导入工作区也要用同一份数据）。 */
  repositories: readonly RepositoryView[];
  class?: string;
}

/** 树里的一行。 */
interface TreeRow {
  name: string;
  /** 库内相对路径（`photos/2026-08-15`）。 */
  relPath: string;
  depth: number;
  expanded: boolean;
  hasChildren: boolean;
}

export function BrowseLeftColumn(props: BrowseLeftColumnProps) {
  const store = props.store;
  const [search, setSearch] = createSignal("");
  const [expandedLibs, setExpandedLibs] = createSignal(false);
  /** 展开的目录（库内相对路径）。 */
  const [expanded, setExpanded] = createSignal<ReadonlySet<string>>(new Set<string>());
  /** 每个目录的子目录（懒加载：展开时才读那一级）。 */
  const [children, setChildren] = createSignal<ReadonlyMap<string, string[]>>(new Map());
  /** 本次会话里的库顺序（点库移顶）。 */
  const [order, setOrder] = createSignal<readonly string[]>([]);

  const root = createMemo(
    () => props.repositories.find((r) => r.id === store.repositoryId())?.root ?? null,
  );

  /** 当前库的卡片（别的库不显示在紧缩视图里）。 */
  const ordered = createMemo(() => {
    const byId = new Map(props.repositories.map((r) => [r.id, r]));
    const seen = order().filter((id) => byId.has(id));
    const rest = props.repositories.map((r) => r.id).filter((id) => !seen.includes(id));
    return [...seen, ...rest].map((id) => byId.get(id)!);
  });

  const filtered = createMemo(() => {
    const needle = search().trim().toLowerCase();
    if (needle === "") return ordered();
    // 搜索条第一版**只搜库名与目录名**（`BROWSE.md` §4.1）
    return ordered().filter((repo) => repo.name.toLowerCase().includes(needle));
  });

  /** 目录名搜索命中的路径（用于目录树过滤）。 */
  const needle = () => search().trim().toLowerCase();

  /** 把绝对路径换算成库内相对路径（统一用 `/`）。 */
  function toRelPath(abs: string): string {
    const base = root();
    if (base === null) return abs;
    const normalized = abs.replace(/\\/g, "/");
    const baseNorm = base.replace(/\\/g, "/").replace(/\/+$/, "");
    return normalized.startsWith(`${baseNorm}/`)
      ? normalized.slice(baseNorm.length + 1)
      : normalized;
  }

  /** 读某一级的子目录（懒加载，读到的存进 `children`）。 */
  async function loadChildren(relPath: string): Promise<void> {
    const base = root();
    if (base === null) return;
    const abs = relPath === "" ? base : `${base.replace(/\/+$/, "")}/${relPath}`;
    try {
      const entries = await listDirs(abs);
      setChildren((prev) => {
        const next = new Map(prev);
        next.set(
          relPath,
          entries.map((e) => toRelPath(e.path)),
        );
        return next;
      });
    } catch {
      // 读不到就当作没有子目录（权限/离线都是常事，不该弹错）
      setChildren((prev) => new Map(prev).set(relPath, []));
    }
  }

  // 换库：清空展开状态并读根级目录
  createEffect(() => {
    const base = root();
    setExpanded(new Set<string>());
    setChildren(new Map());
    if (base !== null) void loadChildren("");
  });

  /** 展开的树（前序遍历）。 */
  const treeRows = createMemo<TreeRow[]>(() => {
    const rows: TreeRow[] = [];
    const walk = (parent: string, depth: number): void => {
      const kids = children().get(parent) ?? [];
      for (const rel of kids) {
        const name = rel.split("/").pop() ?? rel;
        const isExpanded = expanded().has(rel);
        const known = children().get(rel);
        rows.push({
          name,
          relPath: rel,
          depth,
          expanded: isExpanded,
          // 还没读过的那一级一律显示箭头（有没有子目录要读了才知道）
          hasChildren: known === undefined || known.length > 0,
        });
        if (isExpanded) walk(rel, depth + 1);
      }
    };
    walk("", 0);
    const filter = needle();
    if (filter === "") return rows;
    // 目录名过滤：命中即显示（父链在树里天然保留）
    return rows.filter((row) => row.name.toLowerCase().includes(filter));
  });

  function toggleExpand(relPath: string): void {
    const isExpanded = expanded().has(relPath);
    setExpanded((prev) => {
      const next = new Set(prev);
      if (isExpanded) next.delete(relPath);
      else next.add(relPath);
      return next;
    });
    // 展开时读那一级（`BROWSE.md` §4.3：展开只跟用户操作相关）
    if (!isExpanded && children().get(relPath) === undefined) {
      void loadChildren(relPath);
    }
  }

  function selectRepository(id: string): void {
    // 展开态下点库会移顶（`BROWSE.md` §4.2）；收缩态下不动位置
    if (expandedLibs()) {
      setOrder((prev) => [id, ...prev.filter((x) => x !== id)]);
    }
    store.setRepository(id);
  }

  const visibleRepos = () => filtered().slice(0, expandedLibs() ? undefined : COMPACT_REPO_LIMIT);
  const hasMore = () => !expandedLibs() && filtered().length > COMPACT_REPO_LIMIT;

  return (
    <div class={["flex min-h-0 flex-col gap-2 p-2", props.class ?? ""].filter(Boolean).join(" ")}>
      {/* 搜索条 */}
      <input
        type="search"
        value={search()}
        onInput={(event) => setSearch(event.currentTarget.value)}
        placeholder={t("browse.searchPlaceholder")}
        class="h-7 shrink-0 rounded-(--radius) bg-surface-bar px-2 text-fs-2 text-fg-1 placeholder:text-fg-3"
      />

      {/* 紧缩库列表 */}
      <div class="shrink-0">
        <Show when={filtered().length === 0}>
          <p class="px-1 py-2 text-fs-2 text-fg-3">{t("browse.noRepository")}</p>
        </Show>
        <For each={visibleRepos()}>
          {(repo) => (
            <button
              type="button"
              onClick={() => selectRepository(repo.id)}
              class={[
                "mb-1 flex w-full items-center gap-2 rounded-(--radius) px-2 py-1 text-left",
                store.repositoryId() === repo.id
                  ? "bg-state-selected text-fg-1"
                  : "text-fg-2 hover:bg-state-hover",
              ].join(" ")}
            >
              <span class="shrink-0 text-fs-3 text-brand">●</span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-fs-2">{repo.name}</span>
                <span class="block truncate text-fs-0 text-fg-3">
                  {shortPath(repo.displayPath, { maxLength: 48 })}
                </span>
              </span>
              <Show when={!repo.online}>
                <span class="shrink-0 text-fs-0 text-danger">{t("browse.offline")}</span>
              </Show>
            </button>
          )}
        </For>
        <Show when={hasMore()}>
          {/* 3.5 截断的那半截条：只写「查看所有库」 */}
          <button
            type="button"
            onClick={() => setExpandedLibs(true)}
            class="flex h-6 w-full items-center justify-center rounded-(--radius) bg-surface-bar text-fs-0 text-fg-2 hover:bg-state-hover"
          >
            {t("browse.allRepositories")}
          </button>
        </Show>
        <Show when={expandedLibs()}>
          <button
            type="button"
            onClick={() => setExpandedLibs(false)}
            class="flex h-6 w-full items-center justify-center rounded-(--radius) text-fs-0 text-fg-3 hover:bg-state-hover"
          >
            {t("browse.collapseRepositories")}
          </button>
        </Show>
      </div>

      {/* 库内目录树（展开库列表时让位，见 BROWSE.md §4.2） */}
      <Show when={!expandedLibs() && root() !== null}>
        <div class="min-h-0 flex-1 overflow-y-auto">
          <button
            type="button"
            onClick={() => store.setScope(null)}
            class={[
              "mb-1 flex w-full items-center gap-2 rounded-(--radius) px-2 py-1 text-left text-fs-2",
              store.scopePath() === null
                ? "bg-state-selected text-fg-1"
                : "text-fg-2 hover:bg-state-hover",
            ].join(" ")}
          >
            <span class="text-fs-3">◉</span>
            {t("browse.wholeRepository")}
          </button>

          <Show when={treeRows().length === 0}>
            <p class="px-1 py-2 text-fs-2 text-fg-3">{t("browse.emptyTree")}</p>
          </Show>

          <For each={treeRows()}>
            {(row) => (
              <div
                class={[
                  "flex items-center gap-1 rounded-(--radius) pr-1",
                  store.scopePath() === row.relPath
                    ? "bg-state-selected text-fg-1"
                    : "text-fg-2 hover:bg-state-hover",
                ].join(" ")}
                style={{ "padding-left": `${row.depth * 12 + 4}px` }}
              >
                {/* 点箭头 = 展开；点名字 = 选中（BROWSE.md §4.3 的两个动作） */}
                <button
                  type="button"
                  aria-label={row.expanded ? t("browse.collapse") : t("browse.expand")}
                  onClick={(event) => {
                    event.stopPropagation();
                    toggleExpand(row.relPath);
                  }}
                  class="w-4 shrink-0 text-center text-fs-0 text-fg-3 hover:text-fg-1"
                >
                  {row.hasChildren ? (row.expanded ? "▾" : "▸") : ""}
                </button>
                <button
                  type="button"
                  onClick={() => store.setScope(row.relPath)}
                  class="min-w-0 flex-1 truncate py-1 text-left text-fs-2"
                  title={row.relPath}
                >
                  {row.name}
                </button>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 右列：信息栏（tiles 模式）
 * ══════════════════════════════════════════════════════════════ */

export interface AssetInfoProps {
  /** 当前锚点那张（多选时是它，见 `BROWSE.md` §5.10）。 */
  item: AssetItem | null;
  class?: string;
}

/** 一行「字段名 + 值」。 */
function Field(props: { label: string; value: string | null }) {
  return (
    <Show when={props.value !== null && props.value !== ""}>
      <div class="flex items-baseline gap-2">
        <span class="w-16 shrink-0 text-fs-0 text-fg-3">{props.label}</span>
        <span class="min-w-0 flex-1 break-all text-fs-2 text-fg-1">{props.value}</span>
      </div>
    </Show>
  );
}

/** 把毫秒按给定时区偏移渲染成人能读的串。 */
function localDateTime(ms: number, offsetMinutes: number | null): string {
  const date = new Date(ms + (offsetMinutes ?? 0) * 60_000);
  const pad = (n: number): string => String(n).padStart(2, "0");
  return (
    `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())} ` +
    `${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}:${pad(date.getUTCSeconds())}`
  );
}

/** 文件大小（人类可读）。 */
function humanSize(bytes: number | null): string | null {
  if (bytes === null || !Number.isFinite(bytes)) return null;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

/** 曝光时间：1/250 这种写法比 0.004s 直观。 */
function exposureText(ms: number | null): string | null {
  if (ms === null || !Number.isFinite(ms) || ms <= 0) return null;
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)} s`;
  return `1/${Math.round(1000 / ms)} s`;
}

export function AssetInfo(props: AssetInfoProps) {
  const item = () => props.item;
  const camera = () => {
    const it = item();
    if (it === null) return null;
    const text = [it.cameraMake, it.cameraModel].filter(Boolean).join(" ").trim();
    return text === "" ? null : text;
  };

  return (
    <div class={["min-h-0 flex-1 overflow-y-auto p-2", props.class ?? ""].filter(Boolean).join(" ")}>
      <Show
        when={item() !== null}
        fallback={<p class="p-2 text-fs-2 text-fg-3">{t("browse.noSelection")}</p>}
      >
        {/* EXIF（BROWSE.md §6：tiles 模式下内容可能很长，要能滚） */}
        <section class="mb-5">
          <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.exif")}</h3>
          <div class="flex flex-col gap-1.5">
            <Field label={t("browse.fieldCamera")} value={camera()} />
            <Field label={t("browse.fieldLens")} value={item()!.lens} />
            <Field
              label={t("browse.fieldFocal")}
              value={item()!.focalMm === null ? null : `${item()!.focalMm} mm`}
            />
            <Field
              label={t("browse.fieldAperture")}
              value={item()!.fNumber === null ? null : `f/${item()!.fNumber}`}
            />
            <Field
              label={t("browse.fieldExposure")}
              value={exposureText(item()!.exposureMs)}
            />
            <Field
              label={t("browse.fieldIso")}
              value={item()!.iso === null ? null : `ISO ${item()!.iso}`}
            />
            <Field
              label={t("browse.fieldSize")}
              value={
                item()!.width === null || item()!.height === null
                  ? null
                  : `${item()!.width} × ${item()!.height}`
              }
            />
          </div>
        </section>

        {/* 文件信息（作者/描述/地理在 W2 接编辑） */}
        <section class="mb-5">
          <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.fileInfo")}</h3>
          <div class="flex flex-col gap-1.5">
            <Field label={t("browse.fieldFileName")} value={item()!.fileName} />
            <Field
              label={t("browse.fieldTakenAt")}
              value={
                item()!.takenAt === null
                  ? null
                  : localDateTime(item()!.takenAt!, item()!.takenAtOffsetMin)
              }
            />
            <Field label={t("browse.fieldPath")} value={item()!.relPath} />
            <Field
              label={t("browse.fieldFileType")}
              value={item()!.isRaw ? t("browse.raw") : item()!.ext.toUpperCase()}
            />
            <Field label={t("browse.fieldFileSize")} value={humanSize(item()!.sizeBytes)} />
            <Show when={item()!.missing}>
              <p class="text-fs-2 text-danger">{t("browse.missingFile")}</p>
            </Show>
          </div>
        </section>
      </Show>
    </div>
  );
}
