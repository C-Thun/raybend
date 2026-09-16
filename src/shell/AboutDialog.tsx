/**
 * 「帮助 → 关于」的内容（`design/main.md` §2.1：菜单里只有这一项）。
 *
 * 信息分层（用户 2026-09-15 的评审意见：**美观现代、必要信息齐全、开发环境带更多调试信息**）：
 *
 *   ┌───────────────┐
 *   │  ◉  光伴        │  ← 品牌块：图标 + 名称 + 版本标签
 *   │     RayBend     │
 *   │  0.1.0 · Dev    │
 *   ├───────────────┤
 *   │  许可  AGPL-3.0-only   │  ← 必要信息：两列对齐的键值表
 *   │  仓库  github.com/...  │
 *   ├───────────────┤
 *   │  你的照片与目录永远不会被改动   │  ← 一句实话（移除 ≠ 删除）
 *   ├───────────────┤
 *   │  调试信息（仅开发/测试包）       │
 *   └───────────────┘
 *
 * **调试信息只在开发或测试包里出现**：正式包的用户不需要看见 commit hash 与 WebView 版本，
 * 而支持人员需要 —— 所以判定标准是构建通道（`build-info.ts`），不是「有没有开着 devtools」。
 */

import { For, Show } from "solid-js";
import { IconExternalLink } from "@tabler/icons-solidjs";
// 大 logo（带 RayBend 字样的那版）
import logo from "../assets/branding/logo.png";
import { Dialog } from "../components/ui/Dialog.tsx";
import { t } from "../i18n/index.ts";
import {
  channelLabel,
  currentBuildInfo,
  isReleaseReady,
  showsDebugInfo,
  type BuildInfo,
} from "../lib/build-info.ts";

export interface AboutDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 允许测试注入构建信息；省略时用构建期注入的真实值 */
  build?: BuildInfo;
}

const REPOSITORY_URL = "https://github.com/c-thun/raybend";
const LICENSE_ID = "AGPL-3.0-only";

interface InfoRow {
  label: string;
  value: string;
  href?: string;
}

export function AboutDialog(props: AboutDialogProps) {
  const build = () => props.build ?? currentBuildInfo();

  const rows = (): InfoRow[] => [
    { label: t("about.version"), value: build().version },
    {
      label: t("about.license"),
      value: LICENSE_ID,
      href: `${REPOSITORY_URL}/blob/main/LICENSE`,
    },
    {
      label: t("about.repository"),
      value: REPOSITORY_URL.replace("https://", ""),
      href: REPOSITORY_URL,
    },
  ];

  const debugRows = (): InfoRow[] => {
    const rows: InfoRow[] = [];
    if (build().builtAt) {
      // 只显示到分钟：秒对排障没有价值，反而让这一行人眼扫不过去
      rows.push({
        label: t("about.build_time"),
        value: build().builtAt.replace("T", " ").slice(0, 16),
      });
    }
    rows.push({
      label: t("about.runtime"),
      value: runtimeLabel(),
    });
    if (build().gitHash) {
      rows.push({
        label: "commit",
        value: `${build().gitHash}${build().dirty ? " (dirty)" : ""}`,
      });
    }
    return rows;
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("about.title")}
      class="max-w-sm"
    >
      <div class="flex flex-col gap-4">
        {/* ── 品牌块 ─────────────────────────────────── */}
        <div class="flex items-center gap-3">
          {/*
            这里地方大，用**带字**的大 logo（`logo.png`）。
            同样不加 CSS 圆角：徽章自带圆角，再套一层会切边。
          */}
          <img
            src={logo}
            alt=""
            aria-hidden="true"
            draggable={false}
            class="size-10 shrink-0 select-none"
          />
          <div class="min-w-0">
            <div class="truncate text-fs-4 font-semibold text-fg-1">
              {t("app.name")}
            </div>
            <div class="truncate text-fs-1 text-fg-3">
              {build().version} · {channelLabel(build().channel)}
              <Show when={build().dirty && !isReleaseReady(build())}>
                {" "}
                · {t("about.debug")}
              </Show>
            </div>
          </div>
        </div>

        {/* ── 必要信息 ───────────────────────────────── */}
        <InfoTable rows={rows()} />

        {/* ── 一句实话：移除 ≠ 删除 ───────────────────── */}
        <p class="rounded-ui bg-surface-track px-2 py-1.5 text-fs-1 leading-relaxed text-fg-2">
          {t("about.privacy")}
        </p>

        {/* ── 调试信息（仅开发/测试包） ────────────────── */}
        <Show when={showsDebugInfo(build().channel)}>
          <div class="flex flex-col gap-1">
            <span class="text-fs-1 font-medium tracking-wide text-fg-3 uppercase">
              {t("about.debug")}
            </span>
            <InfoTable rows={debugRows()} />
          </div>
        </Show>
      </div>
    </Dialog>
  );
}

function InfoTable(props: { rows: InfoRow[] }) {
  return (
    <dl class="flex flex-col gap-1">
      <For each={props.rows}>
        {(row) => (
          <div class="flex items-baseline gap-2 text-fs-1">
            <dt class="w-16 shrink-0 text-fg-3">{row.label}</dt>
            <dd class="min-w-0 flex-1 truncate text-fg-2">
              <Show when={row.href} fallback={row.value}>
                <a
                  href={row.href}
                  target="_blank"
                  rel="noreferrer noopener"
                  class="inline-flex items-center gap-1 text-fg-2 hover:text-brand"
                >
                  {row.value}
                  <IconExternalLink size={11} aria-hidden="true" />
                </a>
              </Show>
            </dd>
          </div>
        )}
      </For>
    </dl>
  );
}

/** 运行环境一行：浏览器预览 / Tauri（带 WebView 版本，排障时最有用的一条） */
function runtimeLabel(): string {
  const ua = globalThis.navigator?.userAgent ?? "";
  const webview = /(?:Edg|Chrome)\/([\d.]+)/.exec(ua);
  const isTauri = "__TAURI_INTERNALS__" in globalThis;
  if (!isTauri) return t("about.runtime_browser");
  return webview ? `Tauri · WebView ${webview[1]}` : "Tauri";
}
