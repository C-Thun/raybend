/**
 * `TitleBar` —— 应用标题行（`design/main.md` §2.1）。
 *
 * 形态：**沉浸式，没有系统标题行**。窗口拖动与三键都由应用自己接管
 * （`tauri.conf.json` 里 `decorations: false`；拖拽靠 `data-tauri-drag-region`）。
 *
 * 布局（左 → 右）：图标 + 应用名 → 菜单（**悬停才出现**）→ 拖拽空白 → 主题/密度 → 窗口三键。
 *
 * 四件容易做错的事，都在这里处理掉了：
 *
 *   1. **浏览器降级**：`pnpm dev` 在普通浏览器里跑时没有窗口 API。
 *      三键整组**不渲染**（`chrome.view().visible`），而不是渲染出来等它报错。
 *   2. **有系统标题栏时不画三键**：Linux/WSL 的开发配置保留了系统标题栏
 *      （`tauri.linux.conf.json`），那种情况下再画一套按钮只会让人误点。
 *      判定问运行时（`isDecorated()`），不问配置 —— 见 `src/api/window.ts`。
 *   3. **菜单钉住**：菜单在 portal 里，鼠标移进去会触发标题行的 `pointerleave`；
 *      不钉住的话菜单会在被点击的瞬间消失（`shell/store.ts`）。
 *   4. **拖拽区不吃点击**：Tauri 的 drag.js 会让「可点元素」自动阻断拖动，
 *      所以按钮上**不需要**、也不应该加 `data-tauri-drag-region`；
 *      只有标题行容器带这个属性（裸值 = 只有点在它自己身上才拖）。
 *
 * 主题图标表示**当前**模式（深色时显示月亮），点一下切换；密度是两档分段控件。
 */

import { A } from "@solidjs/router";
// Vite 会把图片当资源处理（哈希后进 dist），并给出类型（见 `src/vite-env.d.ts`）
import logoSmall from "../assets/branding/logo-small.png";
import {
  IconArrowsMinimize,
  IconMinus,
  IconMoon,
  IconSquare,
  IconSun,
  IconX,
} from "@tabler/icons-solidjs";
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { createWindowChrome, tauriWindowHandle } from "../api/window.ts";
import { IconButton } from "../components/ui/Button.tsx";
import { Menu } from "../components/ui/Menu.tsx";
import { SegmentedControl } from "../components/ui/SegmentedControl.tsx";
import { Tooltip } from "../components/ui/Tooltip.tsx";
import { locale, localeLabel, nextLocale, t } from "../i18n/index.ts";
import type { MessageKey } from "../i18n/index.ts";
import { availabilityOf, chordOf, type CommandSpec } from "../lib/commands.ts";
import { shortcutOverrides } from "../lib/shortcuts.ts";
import type { AppearanceStore } from "../lib/appearance.ts";
import { AboutDialog } from "./AboutDialog.tsx";
import type { ShellStore } from "./store.ts";

/** 标题栏上的菜单顺序（`specs/M2-W3.md` §2.6） */
const MENU_ORDER = ["file", "edit", "view", "window", "help"] as const;
type MenuName = (typeof MENU_ORDER)[number];

export interface TitleBarProps {
  store: ShellStore;
  appearance: AppearanceStore;
  /** 命令注册表（菜单项**全部**从它来：所以「每个菜单项都是可搜索命令」是结构保证） */
  commands: readonly CommandSpec[];
  /** 跑一条命令（组装层的统入口：记最近 + 真的跑） */
  onRun: (command: CommandSpec) => void;
  /** 关于弹窗的状态住在组装层（命令面板也要能打开它） */
  aboutOpen: boolean;
  onAboutOpenChange: (open: boolean) => void;
}

export function TitleBar(props: TitleBarProps) {
  const chrome = createWindowChrome();
  const [openMenu, setOpenMenu] = createSignal<MenuName | null>(null);

  createEffect(() => props.store.pinMenus(openMenu() !== null));
  onCleanup(() => props.store.pinMenus(false));

  onMount(() => {
    void (async () => {
      await chrome.attach(await tauriWindowHandle());
    })();
  });
  onCleanup(() => chrome.dispose());

  const controls = () => chrome.view();

  /**
   * 菜单标签：一律从注册表取本地化标题 —— **只有一个例外**：
   * 语言切换那条要显示**目标语言自己的名字**（endonym，点下去会变成什么），
   * 用 `t()` 会让中英两种界面都写自己的名字，那条菜单就失去意义了。
   */
  const itemLabel = (command: CommandSpec): string =>
    command.id === "help.language.toggle"
      ? localeLabel(nextLocale(locale()))
      : t(command.titleKey as MessageKey);

  /** 每个菜单的项（顺序 = 注册表顺序） */
  const menuItems = createMemo(() => {
    const build = (name: MenuName) =>
      props.commands
        .filter((command) => command.menu === name)
        .map((command) => ({
          value: command.id,
          label: itemLabel(command),
          // 与命令面板 / 分发器同一份可用性判定（`when` + `enabled`）：暗着但可见
          ...(!availabilityOf(command).available ? { disabled: true } : {}),
          ...(chordOf(command, shortcutOverrides()) === null
            ? {}
            : { shortcut: chordOf(command, shortcutOverrides()) ?? undefined }),
          ...(isActive(command) ? { selected: true } : {}),
        }));
    return new Map<MenuName, ReturnType<typeof build>>(MENU_ORDER.map((name) => [name, build(name)]));
  });

  /** 菜单里的「当前生效项」：工作流 / 主题 / 密度 / 语言那几条打个勾 */
  function isActive(command: CommandSpec): boolean {
    switch (command.id) {
      case "view.flow.import":
      case "view.flow.browse":
      case "view.flow.edit":
      case "view.flow.export":
        return props.store.workflow() === command.id.split(".")[2];
      case "view.density.compact":
        return props.appearance.density() === "compact";
      case "view.density.loose":
        return props.appearance.density() === "loose";
      default:
        return false;
    }
  }

  const runById = (id: string): void => {
    const command = props.commands.find((item) => item.id === id);
    if (command === undefined) return;
    // 语言那条的标签是特例（endonym），行为仍在注册表里
    props.onRun(command);
  };

  return (
    <>
      <header
        // 裸值 = 只有直接点在标题行自身（含空白拖拽区）才触发拖动；
        // 按钮/标签这类可点元素会**自动**阻断拖动，所以不要给它们加这个属性
        data-tauri-drag-region
        /*
         * `z-(--z-titlebar)`：在遮罩之上、弹窗之下（人类 2026-09-25 更新）。
         * `relative` 是让 z 生效的前提（static 元素上的 z-index 会被忽略）。
         */
        class="relative z-(--z-titlebar) flex h-bar-title-h shrink-0 items-stretch bg-surface-bar text-fg-1 select-none"
        onPointerEnter={() => props.store.setMenuHover(true)}
        onPointerLeave={() => props.store.setMenuHover(false)}
        onFocusIn={() => props.store.setMenuFocus(true)}
        onFocusOut={(event) => {
          // 焦点在标题行内部移动时不算「离开」，否则菜单会闪
          const next = event.relatedTarget as Node | null;
          if (!next || !event.currentTarget.contains(next)) {
            props.store.setMenuFocus(false);
          }
        }}
      >
        {/* ── 应用图标 + 名称 ───────────────────────────── */}
        <div data-tauri-drag-region class="flex shrink-0 items-center gap-2 ps-pad-x">
          {/*
            真 logo —— 用 `logo-small.png`（**只有徽章、没有字**的那版）：
            20px 下带字的版本糊成一团，这版才认得出，这是它存在的理由。
            两条注意：
              1. **不要**再加 `rounded-ui`：徽章自己的圆角在 20px 下约 2.4px，
                 再套 4px 的 CSS 圆角会切进徽章边缘（露出底下的条带色）。
              2. 图片要 `draggable={false}`（否则桌面应用里会触发 webview 的「拖文件」），
                 并带上 `data-tauri-drag-region`：点图标也该能拖窗口。
          */}
          <img
            src={logoSmall}
            alt=""
            aria-hidden="true"
            draggable={false}
            data-tauri-drag-region
            class="size-5 shrink-0 select-none"
          />
          <span class="text-fs-3 font-semibold whitespace-nowrap">
            {t("app.name")}
          </span>
        </div>

        {/* ── 菜单：只在鼠标指向标题行时出现（设计稿明确要求）──
            五个菜单（文件 / 编辑 / 视图 / 窗口 / 帮助）的**每一项都来自命令注册表** ——
            所以「菜单项同时是可搜索命令」不是靠人工同步，而是结构保证；
            右侧那颗键位提示也读同一份覆盖表，改完键菜单会跟着变。 */}
        <div class="flex shrink-0 items-center gap-0.5 ps-2">
          <Show when={props.store.menuVisible()}>
            <For each={[...MENU_ORDER]}>
              {(name) => (
                <Menu
                  open={openMenu() === name}
                  label={t(`titlebar.menu.${name}` as MessageKey)}
                  placement="bottom-start"
                  items={menuItems().get(name) ?? []}
                  onSelect={(value) => runById(value)}
                  onOpenChange={(open) =>
                    setOpenMenu((current) => (open ? name : current === name ? null : current))
                  }
                >
                  {(triggerProps) => (
                    <button
                      {...triggerProps()}
                      onPointerEnter={() => {
                        if (openMenu() !== null && openMenu() !== name) setOpenMenu(name);
                      }}
                      class="rounded-ui px-2 py-0.5 text-fs-2 text-fg-2 transition-colors hover:bg-state-hover hover:text-fg-1"
                    >
                      {t(`titlebar.menu.${name}` as MessageKey)}
                    </button>
                  )}
                </Menu>
              )}
            </For>
          </Show>
        </div>

        {/* ── 空白拖拽区 ──────────────────────────────────
            这里的 `data-tauri-drag-region` **必须有**：Tauri 只认「带属性的元素自身」，
            header 上的那一个管不到子元素 —— 去掉它中间这片就按不住（2026-09-16 的实测反馈）。
            它 `flex-1`，所以宽度天然吃满「左侧控件」与「右侧主题/密度/三键」之间的全部空白。 */}
        <div data-tauri-drag-region class="h-full min-w-4 flex-1" />

        {/* ── 开发期入口（生产构建整条分支被摇掉） ──────── */}
        <Show when={import.meta.env.DEV}>
          <A
            href="/dev/kitchen-sink"
            class="flex shrink-0 items-center rounded-ui px-2 text-fs-1 text-fg-3 transition-colors hover:bg-state-hover hover:text-fg-1"
          >
            {/* i18n-exempt: 只在 DEV 构建里存在的陈列室入口，生产包没有这条分支 */}
            组件陈列室
          </A>
        </Show>

        {/* ── 主题 + 密度 ───────────────────────────────── */}
        <div class="flex shrink-0 items-center gap-2 pe-pad-x">
          <Tooltip content={t("titlebar.theme.toggle")}>
            {(triggerProps) => (
              <IconButton
                {...triggerProps()}
                label={t("titlebar.theme.toggle")}
                onClick={() => props.appearance.toggleTheme()}
              >
                <Show
                  when={props.appearance.theme() === "dark"}
                  fallback={<IconSun size={14} />}
                >
                  <IconMoon size={14} />
                </Show>
              </IconButton>
            )}
          </Tooltip>

          <SegmentedControl
            label={t("titlebar.density.compact")}
            value={props.appearance.density()}
            onValueChange={props.appearance.setDensity}
            options={[
              { value: "compact", label: t("titlebar.density.compact") },
              { value: "loose", label: t("titlebar.density.loose") },
            ]}
          />
        </div>

        {/* ── 窗口三键（贴着右边缘；浏览器里整组不存在）──── */}
        <Show when={controls().visible}>
          <div class="flex h-full shrink-0 items-stretch">
            <WindowButton
              label={t("titlebar.window.minimize")}
              onClick={() => void chrome.minimize()}
            >
              <IconMinus size={14} aria-hidden="true" />
            </WindowButton>
            <WindowButton
              label={
                controls().action === "restore"
                  ? t("titlebar.window.restore")
                  : t("titlebar.window.maximize")
              }
              onClick={() => void chrome.toggleMaximize()}
            >
              <Show
                when={controls().action === "restore"}
                fallback={<IconSquare size={11} aria-hidden="true" />}
              >
                <IconArrowsMinimize size={11} aria-hidden="true" />
              </Show>
            </WindowButton>
            <WindowButton
              label={t("titlebar.window.close")}
              danger
              onClick={() => void chrome.close()}
            >
              <IconX size={14} aria-hidden="true" />
            </WindowButton>
          </div>
        </Show>
      </header>

      <AboutDialog open={props.aboutOpen} onOpenChange={props.onAboutOpenChange} />
    </>
  );
}

interface WindowButtonProps {
  label: string;
  danger?: boolean;
  onClick: () => void;
  children: JSX.Element;
}

/**
 * 窗口三键之一。**方正、贴边、撑满标题行高度** —— 与系统标题栏的位置感一致，
 * 所以这里不用圆角、也不留外边距。
 * 关闭键的悬停红来自 `--danger` 令牌（`DESIGN.md` §9.2，用户指定）。
 */
function WindowButton(props: WindowButtonProps) {
  return (
    <button
      type="button"
      aria-label={props.label}
      title={props.label}
      onClick={props.onClick}
      class={[
        "flex h-full w-win-ctl-w shrink-0 items-center justify-center transition-colors",
        props.danger
          ? "text-fg-2 hover:bg-danger hover:text-fg-on-danger"
          : "text-fg-2 hover:bg-state-hover hover:text-fg-1",
      ].join(" ")}
    >
      {props.children}
    </button>
  );
}
