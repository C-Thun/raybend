/** One presentation for editor and device settings; diagnostic details stay optional. */
import { createMemo, Show } from "solid-js";
import { displayColorMode, type DisplayPhase, type DisplayPresentation } from "../../lib/display-color.ts";
import { t } from "../../i18n/index.ts";
import { displayColorCopy } from "../../i18n/display-color.ts";

export function DisplayColorStatus(props: { status: DisplayPresentation | null; phase?: DisplayPhase; prominent?: boolean; error?: string | null }) {
  const copy = createMemo(() => displayColorCopy(displayColorMode(props.status, props.phase)));
  const diagnostic = () => props.error ?? props.status?.diagnostic ?? props.status?.profilePath;
  return <div aria-live="polite">
    <p class={`${props.prominent ? "text-fs-3" : "text-fs-1"} font-semibold ${copy().warning ? "text-brand-2" : "text-fg-1"}`}>{t(copy().title)}</p>
    <p class="mt-1 text-fs-1 leading-relaxed text-fg-2">{t(copy().detail)}</p>
    <Show when={props.status?.sdrWhiteNits}>{nits => <p class="mt-1 text-fs-0 text-fg-3">{t("color.display.sdrWhite", { nits: nits() })}</p>}</Show>
    <Show when={diagnostic()}>{value => <details class="mt-2 text-fs-0 text-fg-3">
      <summary class="w-fit cursor-pointer rounded-ui hover:text-fg-1 focus-visible:outline focus-visible:outline-brand">{t("color.display.diagnostic")}</summary>
      <p class="mt-1 whitespace-pre-wrap break-all select-text">{value()}</p>
    </details>}</Show>
  </div>;
}
