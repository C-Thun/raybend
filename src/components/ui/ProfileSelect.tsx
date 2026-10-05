import { For, Show } from "solid-js";
import { Select } from "./Form.tsx";
import { t } from "../../i18n/index.ts";

/** Presentation fields only; callers adapt their library entries structurally. */
export interface ProfileSelectEntry {
  key: string; profileId: string; name: string; profileClass: string;
  builtIn: boolean; hidden: boolean; available: boolean;
}

/** Input and export share the same role/availability filter and missing selection. */
export function ProfileSelect(props: {
  label: string; value: string; role: "input" | "output"; entries: readonly ProfileSelectEntry[];
  disabled?: boolean; allowAssignment?: boolean; allowAutomatic?: boolean; class?: string; srgbOnly?: boolean; onFocus?: () => void; onChange: (value: string) => void;
}) {
  const entries = () => props.entries.filter(entry => !entry.hidden && entry.available
    && (props.role === "input" ? entry.profileClass !== "output" : entry.profileClass !== "input"));
  const options = () => [...(props.allowAutomatic ? [{key:"auto",name:t("editor.colorManagement.automatic")}] : []), { key: "srgb", name: "sRGB" }, ...entries().filter(entry => entry.key !== "builtin:srgb-v1").map(entry => ({
    key: props.role === "output" && entry.builtIn
      ? entry.key === "builtin:display-p3-v1" ? "display_p3" : "adobe_rgb" : entry.profileId,
    name: entry.name,
  })), ...(props.allowAssignment ? [{ key: "require_assignment", name: t("settings.input.require") }] : [])];
  return <Select aria-label={props.label} value={props.value} disabled={props.disabled} onFocus={props.onFocus} class={props.class ?? "w-44 shrink-0"} onChange={event => props.onChange(event.currentTarget.value)}>
    <Show when={!options().some(option => option.key === props.value)}>
      <option value={props.value} disabled>{props.entries.find(entry => entry.profileId === props.value)?.name ?? `${t("settings.profiles.missing")}: ${props.value.slice(0, 12)}`}</option>
    </Show>
    <For each={options()}>{option => <option value={option.key} disabled={props.srgbOnly && option.key !== "srgb"}>{option.name}</option>}</For>
  </Select>;
}
