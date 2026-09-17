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
import { IconDots, IconFolderMinus, IconFolderPlus } from "@tabler/icons-solidjs";

import { dirCreate, dirEmptyCheck, dirRemoveEmpty, listDirs } from "../../api/db.ts";
import { Button } from "../../components/ui/Button.tsx";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { Menu } from "../../components/ui/Menu.tsx";
import type { AssetItem, DirEmptyView, RepositoryView } from "../../api/types.ts";
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

  /* ══ 行尾 `⋯` 菜单的两个动作（`BROWSE.md` §4.3）══ */

  /** 每个目录的**深度**空检查结果，菜单打开时查一次。 */
  const [emptyInfo, setEmptyInfo] = createSignal<ReadonlyMap<string, DirEmptyView>>(new Map());
  /** 待确认删除的目录（null = 没弹） */
  const [confirmRel, setConfirmRel] = createSignal<string | null>(null);
  /** 正在建子目录的**父**目录（null = 没弹） */
  const [createRel, setCreateRel] = createSignal<string | null>(null);
  const [newName, setNewName] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [formError, setFormError] = createSignal<string | null>(null);

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

  /* ══ 行尾 `⋯` 的实际动作 ══ */

  /**
   * 查一次这棵子树里有没有文件 —— 「删除空目录」能不能点**由它决定**。
   *
   * 查不动（离线、权限、目录刚被别人删了）时按**不空**处理：菜单项禁用，
   * 总比让人点了再报错好。
   */
  async function checkEmpty(relPath: string): Promise<void> {
    const base = root();
    if (base === null) return;
    try {
      const info = await dirEmptyCheck(base, relPath);
      setEmptyInfo((prev) => new Map(prev).set(relPath, info));
    } catch {
      setEmptyInfo((prev) =>
        new Map(prev).set(relPath, {
          empty: false,
          fileCount: 0,
          dirCount: 0,
          emptyDirCount: 0,
          hasUnresolvedLink: false,
        }),
      );
    }
  }

  /** 父目录的库内相对路径（`photos/2026` → `photos`；根级 → `""`）。 */
  function parentOf(relPath: string): string {
    const cut = relPath.lastIndexOf("/");
    return cut === -1 ? "" : relPath.slice(0, cut);
  }

  /** 把这个目录（及其后代）从展开态与子目录缓存里清掉 —— 它已经不在磁盘上了。 */
  function forget(relPath: string): void {
    const gone = (key: string): boolean => key === relPath || key.startsWith(`${relPath}/`);
    setExpanded((prev) => {
      const next = new Set(prev);
      for (const key of [...next]) if (gone(key)) next.delete(key);
      return next;
    });
    setChildren((prev) => {
      const next = new Map(prev);
      for (const key of [...next.keys()]) if (gone(key)) next.delete(key);
      return next;
    });
  }

  /** 删掉一棵空目录树，然后把树刷成新的样子。 */
  async function doDeleteEmpty(): Promise<void> {
    const relPath = confirmRel();
    const base = root();
    if (relPath === null || base === null) return;
    setBusy(true);
    setFormError(null);
    try {
      await dirRemoveEmpty(base, relPath);
      const parent = parentOf(relPath);
      forget(relPath);
      // 正在看的目录被删了 ⇒ 退回「整个库」，否则网格会停在一个不存在的范围上
      const scope = store.scopePath();
      if (scope !== null && (scope === relPath || scope.startsWith(`${relPath}/`))) {
        store.setScope(null);
      }
      await loadChildren(parent);
      setConfirmRel(null);
    } catch (error) {
      setFormError(t("browse.deleteDirFailed", { message: String(error) }));
    } finally {
      setBusy(false);
    }
  }

  /** 建一个子目录：展开父目录、读出那一级、并选中新建的目录。 */
  async function doCreateSubdir(): Promise<void> {
    const parentRel = createRel();
    const base = root();
    if (parentRel === null || base === null) return;
    setBusy(true);
    setFormError(null);
    try {
      const created = await dirCreate(base, parentRel, newName().trim());
      if (parentRel !== "") setExpanded((prev) => new Set(prev).add(parentRel));
      await loadChildren(parentRel);
      store.setScope(created);
      setCreateRel(null);
      setNewName("");
    } catch (error) {
      setFormError(t("browse.createFailed", { message: String(error) }));
    } finally {
      setBusy(false);
    }
  }

  /** `⋯` 菜单里那一行给菜单项用的禁用判定。 */
  const canDeleteEmpty = (relPath: string): boolean =>
    emptyInfo().get(relPath)?.empty === true;

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

                {/*
                  行尾 `⋯`（`BROWSE.md` §4.3）：删除空目录 / 创建子目录。

                  空检查是**打开菜单时**才做的（深度检索要碰盘），结果缓存起来决定
                  「删除空目录」能不能点；非空时菜单项在，但是暗的。
                */}
                <Menu
                  label={t("browse.dirMenu")}
                  placement="bottom-end"
                  items={[
                    {
                      value: "delete-empty",
                      label: t("browse.deleteEmptyDir"),
                      icon: <IconFolderMinus size={14} />,
                      disabled: !canDeleteEmpty(row.relPath),
                    },
                    {
                      value: "create-subdir",
                      label: t("browse.createSubdir"),
                      icon: <IconFolderPlus size={14} />,
                    },
                  ]}
                  onSelect={(value) => {
                    setFormError(null);
                    if (value === "delete-empty") setConfirmRel(row.relPath);
                    if (value === "create-subdir") {
                      setNewName("");
                      setCreateRel(row.relPath);
                    }
                  }}
                  onOpenChange={(open) => {
                    if (open) void checkEmpty(row.relPath);
                  }}
                >
                  {(triggerProps) => (
                    <button
                      {...triggerProps()}
                      aria-label={t("browse.dirMenu")}
                      class="shrink-0 rounded-(--radius) px-1 text-fg-3 hover:bg-state-hover hover:text-fg-1"
                    >
                      <IconDots size={14} />
                    </button>
                  )}
                </Menu>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/*
        删除空目录的确认。

        这里是**真的在磁盘上删目录**（不是「从集合里移除」），所以刻意**不做**
        `easy destroy` 的 Shift 快通道（`DESIGN.md` §12.2 那条是为移除类操作定的）。
        能删的东西本身无害（目录里一个文件都没有 → 删了不影响照片），
        但一旦删错就无法撤销，弹一次值得。
      */}
      <ConfirmDialog
        open={confirmRel() !== null}
        title={t("browse.deleteEmptyTitle")}
        message={[
          t("browse.deleteEmptyBody", {
            n: confirmRel() === null ? 0 : (emptyInfo().get(confirmRel()!)?.emptyDirCount ?? 0),
          }),
          // 删除失败（目录被别人占用/中途冒出了文件）时把话接在正文后面：
          // 这个弹窗没有单独的错误行，报错不能没地方去
          formError(),
        ]
          .filter((line): line is string => line !== null && line !== "")
          .join("\n\n")}
        confirmLabel={t("browse.deleteEmptyDir")}
        onConfirm={() => void doDeleteEmpty()}
        onCancel={() => {
          setConfirmRel(null);
          setFormError(null);
        }}
      />

      {/* 创建子目录（重名/非法名字的错误由 Rust 侧挡住，这里只把话说给用户） */}
      <Dialog
        open={createRel() !== null}
        onOpenChange={(open) => {
          if (!open) {
            setCreateRel(null);
            setFormError(null);
          }
        }}
        title={t("browse.createSubdirTitle")}
        footer={
          <>
            <Button
              variant="secondary"
              onClick={() => {
                setCreateRel(null);
                setFormError(null);
              }}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={busy() || newName().trim() === ""}
              onClick={() => void doCreateSubdir()}
            >
              {t("browse.createSubdir")}
            </Button>
          </>
        }
      >
        <label class="flex flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("browse.subdirName")}</span>
          <Input
            value={newName()}
            placeholder={t("browse.subdirNamePlaceholder")}
            onInput={(event) => setNewName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && newName().trim() !== "") void doCreateSubdir();
            }}
          />
        </label>
        <Show when={createRel() !== null}>
          <p class="text-fs-0 text-fg-3">
            {createRel() === "" ? t("browse.wholeRepository") : createRel()}
          </p>
        </Show>
        <Show when={formError() !== null}>
          <p class="text-fs-0 text-fg-2">{formError()}</p>
        </Show>
      </Dialog>
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
