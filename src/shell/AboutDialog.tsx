/** 关于、离线文档与第三方许可共用一个窗口。 */
import { For, Show, createEffect, createSignal } from "solid-js";
import { Button } from "../components/ui/Button.tsx";
import { Input } from "../components/ui/Form.tsx";
import { locale } from "../i18n/index.ts";
import helpCN from "../../docs/user-guide.md?raw";
import helpEN from "../../docs/user-guide.en.md?raw";
import privacy from "../../docs/privacy.md?raw";
import privacyEN from "../../docs/privacy.en.md?raw";
import ownLicense from "../../LICENSE?raw";
import { diagnosticSummary } from "../lib/support-info.ts";
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
  page?: "about" | "help" | "licenses";
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
  const [page,setPage]=createSignal<"about"|"help"|"licenses">("about");
  const [search,setSearch]=createSignal("");
  const [components,setComponents]=createSignal<Array<{name:string;version:string;license:string;source:string;texts:Array<{file:string;text:string;source?:string}>}>>([]);
  const [licenseStatus,setLicenseStatus]=createSignal<"idle"|"loading"|"ready"|"error">("idle");
  const [copyStatus,setCopyStatus]=createSignal("");
  createEffect(()=>{if(props.open)setPage(props.page??"about");});
  createEffect(()=>{
    if(!props.open || page()!=="licenses" || licenseStatus()!=="idle")return;
    setLicenseStatus("loading");
    void fetch("/legal/third-party.json").then(response=>{if(!response.ok)throw new Error("license resource missing");return response.json();}).then(data=>{if(data.schema!==1 || !Array.isArray(data.components))throw new Error("license resource invalid");setComponents(data.components);setLicenseStatus("ready");}).catch(()=>setLicenseStatus("error"));
  });
  const filtered=()=>{const value=search().trim().toLocaleLowerCase();return components().filter(p=>`${p.name} ${p.version} ${p.license}`.toLocaleLowerCase().includes(value));};
  async function copy():Promise<void>{try{await navigator.clipboard.writeText(diagnosticSummary(build(),navigator.userAgent));setCopyStatus(t("about.copied"));}catch{setCopyStatus(t("about.copy_failed"));}}


  const rows = (): InfoRow[] => [
    { label: t("about.version"), value: build().version },
    {
      label: t("about.license"),
      value: LICENSE_ID,
      href: `${REPOSITORY_URL}/blob/master/LICENSE`,
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
      size="wide"
      footer={<Button variant="secondary" onClick={()=>props.onOpenChange(false)}>{t("common.close")}</Button>}
    >
      <div class="flex flex-col gap-4">
        <div class="flex flex-wrap gap-1">
          <For each={["about","help","licenses"] as const}>{value=><Button variant={page()===value?"primary":"secondary"} onClick={()=>setPage(value)}>{t(`about.page.${value}`)}</Button>}</For>
        </div>
        <Show when={page()==="about"}>

        {/* ── 品牌块 ─────────────────────────────────── */}
        <div class="flex items-center gap-4">
          {/*
            这里地方大，用**带字**的大 logo（`logo.png`）。
            同样不加 CSS 圆角：徽章自带圆角，再套一层会切边。
          */}
          <img
            src={logo}
            alt=""
            aria-hidden="true"
            draggable={false}
            class="size-20 shrink-0 select-none"
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
        <Button variant="secondary" onClick={()=>void copy()}>{t("about.copy_diagnostic")}</Button>
        <p aria-live="polite" class="text-fs-1 text-fg-3">{copyStatus()}</p>
        <details class="text-fs-1"><summary class="cursor-pointer text-fg-2">{t("about.privacy_title")}</summary><pre class="mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-sans text-fg-2">{locale()==="en-US"?privacyEN:privacy}</pre></details>
        </Show>
        <Show when={page()==="help"}><pre class="max-h-[60vh] overflow-auto whitespace-pre-wrap font-sans text-fs-1 leading-relaxed text-fg-2">{locale()==="en-US"?helpEN:helpCN}</pre></Show>
        <Show when={page()==="licenses"}>
          <details class="text-fs-1"><summary class="cursor-pointer text-fg-2">AGPL-3.0-only</summary><pre class="max-h-48 overflow-auto whitespace-pre-wrap text-fs-0">{ownLicense}</pre></details>
          <Input aria-label={t("about.license_search")} placeholder={t("about.license_search")} value={search()} onInput={e=>setSearch(e.currentTarget.value)}/>
          <Show when={licenseStatus()==="loading"}><p class="text-fs-1 text-fg-3">{t("common.loading")}</p></Show>
          <Show when={licenseStatus()==="error"}><p role="alert" class="text-fs-1 text-danger">{t("about.license_error")}</p><Button onClick={()=>setLicenseStatus("idle")}>{t("common.retry")}</Button></Show>
          <div class="max-h-[50vh] overflow-auto"><For each={filtered()}>{component=><details class="border-b border-surface-track py-2 text-fs-1">
            <summary class="cursor-pointer break-words text-fg-2">{component.name} {component.version} · {component.license}</summary>
            <a href={component.source} target="_blank" rel="noreferrer noopener" class="break-all text-brand">{component.source}</a>
            <For each={component.texts}>{text=><div><span class="text-fg-3">{text.file}</span><pre class="whitespace-pre-wrap text-fs-0 leading-relaxed">{text.text}</pre></div>}</For>
          </details>}</For></div>
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
