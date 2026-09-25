/** Shared name/value rows for the editor and browse metadata panels. */
import { For, Show, type JSX } from "solid-js";
import { EasyCopy } from "./EasyCopy.tsx";

export interface MetadataRow {
  label: string;
  value: string | null;
}

export function MetadataRows(props: {
  rows: readonly MetadataRow[];
  labelWidth?: "wide" | "compact";
}): JSX.Element {
  return (
    <div class="flex flex-col gap-1.5" data-metadata-rows>
      <For each={props.rows}>
        {(row) => (
          <Show when={row.value !== null && row.value !== ""}>
            <div class="flex items-baseline gap-2" data-metadata-row>
              <span
                class={[
                  props.labelWidth === "compact" ? "w-16" : "w-20",
                  "shrink-0 break-words text-fs-0 text-fg-3",
                ].join(" ")}
              >
                {row.label}
              </span>
              <EasyCopy value={row.value!} class="min-w-0 flex-1">
                <span class="min-w-0 break-words text-fs-1 text-fg-1">{row.value}</span>
              </EasyCopy>
            </div>
          </Show>
        )}
      </For>
    </div>
  );
}
