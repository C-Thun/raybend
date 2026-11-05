/**
 * **控制点容差映射的调试面板**（只有 DEV 构建里存在这个入口）。
 *
 * ```text
 * titlebar                    [🐞] [☀/🌙] [紧凑|宽松] [— □ ✕]
 *                                 ↑ 这个按钮只在 import.meta.env.DEV 下渲染
 * ```
 *
 * # 为什么要有它
 *
 * 容差的手感（亮度放多宽、色相收多紧、白线画多长）是**调出来的**，不是算出来的：
 * 每次改常量都要重新编译一次 Windows 产物，试错成本太高。这个弹窗把参数直接暴露出来，
 * 改一下立刻重出帧，调好了再把数字固化回 `develop/areas.rs::POINT_WEIGHT_CONFIG`。
 *
 * # 边界（崔总 2026-11-01 定）
 *
 * * **入口与框架固定，内容随开发需要变**：换一组旋钮时只改本文件，别动入口；
 * * **生产构建整条分支被摇掉**（`import.meta.env.DEV` 守卫 + 惰性加载 + 模块内
 *   `lib/mask-debug.ts` 是唯一被引用的东西，载荷里 `maskDebug` 恒为 `undefined`）；
 * * 只影响**编辑器真帧**（缩略图 / 导出 / 定稿走固化常量）。
 */

import { For, Show, createMemo, createSignal, onMount, type JSX } from "solid-js";

import { Button } from "../components/ui/Button.tsx";
import { Dialog } from "../components/ui/Dialog.tsx";
import { t } from "../i18n/index.ts";
import type { MessageKey } from "../i18n/index.ts";
import {
  MASK_LINE_DEFAULT,
  maskDebug,
  setMaskDebug,
  type MaskDebugState,
} from "../lib/mask-debug.ts";
import { getMaskDefaults } from "../api/editor.ts";

type PairKey = "lumaCore" | "lumaEnd" | "chromaCore" | "chromaEnd";

interface RowSpec {
  key: PairKey | "line";
  label: MessageKey;
  hint: MessageKey;
  step: number;
}

const ROWS: readonly RowSpec[] = [
  { key: "lumaCore", label: "debug.mask.lumaCore", hint: "debug.mask.lumaCoreHint", step: 0.005 },
  { key: "lumaEnd", label: "debug.mask.lumaEnd", hint: "debug.mask.lumaEndHint", step: 0.005 },
  { key: "chromaCore", label: "debug.mask.chromaCore", hint: "debug.mask.chromaCoreHint", step: 0.005 },
  { key: "chromaEnd", label: "debug.mask.chromaEnd", hint: "debug.mask.chromaEndHint", step: 0.005 },
  { key: "line", label: "debug.mask.line", hint: "debug.mask.lineHint", step: 1 },
];

export interface MaskDebugDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function MaskDebugDialog(props: MaskDebugDialogProps): JSX.Element {
  /** 当前编辑中的值（没存过就先用默认；挂载时去 Rust 取真默认再填） */
  const [draft, setDraft] = createSignal<MaskDebugState | null>(maskDebug());
  const [busy, setBusy] = createSignal(false);

  onMount(() => {
    if (draft() !== null) return;
    void loadDefaults();
  });

  async function loadDefaults(): Promise<void> {
    setBusy(true);
    try {
      const defaults = await getMaskDefaults();
      setDraft({
        lumaCore: [...defaults.lumaCore] as [number, number],
        lumaEnd: [...defaults.lumaEnd] as [number, number],
        chromaCore: [...defaults.chromaCore] as [number, number],
        chromaEnd: [...defaults.chromaEnd] as [number, number],
        lineMin: MASK_LINE_DEFAULT.min,
        lineMax: MASK_LINE_DEFAULT.max,
      });
    } finally {
      setBusy(false);
    }
  }

  /** 写一个数：**立刻生效**（载荷一变就重出帧），并落盘。 */
  function update(key: PairKey, index: 0 | 1, value: number): void {
    const current = draft();
    if (current === null || !Number.isFinite(value)) return;
    const pair = [...current[key]] as [number, number];
    pair[index] = value;
    const next = { ...current, [key]: pair };
    setDraft(next);
    setMaskDebug(next);
  }

  function updateLine(index: 0 | 1, value: number): void {
    const current = draft();
    if (current === null || !Number.isFinite(value)) return;
    const next = {
      ...current,
      lineMin: index === 0 ? Math.max(8, value) : current.lineMin,
      lineMax: index === 1 ? Math.max(current.lineMin + 8, value) : current.lineMax,
    };
    setDraft(next);
    setMaskDebug(next);
  }

  const effective = createMemo(() => draft());

  const pairValue = (key: PairKey, index: 0 | 1): number => effective()?.[key][index] ?? 0;

  return (
    <Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("debug.mask.title")}
      description={t("debug.mask.description")}
      size="wide"
      footer={
        <>
          <Button
            variant="secondary"
            onClick={() => {
              setBusy(true);
              void getMaskDefaults().then((defaults) => {
                const state: MaskDebugState = {
                  lumaCore: [...defaults.lumaCore] as [number, number],
                  lumaEnd: [...defaults.lumaEnd] as [number, number],
                  chromaCore: [...defaults.chromaCore] as [number, number],
                  chromaEnd: [...defaults.chromaEnd] as [number, number],
                  lineMin: MASK_LINE_DEFAULT.min,
                  lineMax: MASK_LINE_DEFAULT.max,
                };
                setDraft(state);
                setMaskDebug(state);
                setBusy(false);
              });
            }}
            disabled={busy()}
          >
            {t("debug.mask.reset")}
          </Button>
          <Button variant="ghost" disabled={busy()} onClick={() => { setMaskDebug(null); void loadDefaults(); }}>
            {t("debug.mask.useBuiltin")}
          </Button>
          <Button variant="primary" onClick={() => props.onOpenChange(false)}>
            {t("common.close")}
          </Button>
        </>
      }
    >
      <div class="flex flex-col gap-3" data-mask-debug>
        <p class="text-fs-0 leading-snug text-fg-3">{t("debug.mask.devOnly")}</p>
        <For each={ROWS}>
          {(row) => (
            <div class="flex items-center gap-3" data-mask-debug-row={row.key}>
              <div class="flex min-w-0 flex-1 flex-col">
                <span class="text-fs-1 text-fg-1">{t(row.label)}</span>
                <span class="text-fs-0 text-fg-3">{t(row.hint)}</span>
              </div>
              <Show
                when={row.key !== "line"}
                fallback={
                  <>
                    <NumberField
                      value={effective()?.lineMin ?? 0}
                      step={row.step}
                      onInput={(value) => updateLine(0, value)}
                    />
                    <NumberField
                      value={effective()?.lineMax ?? 0}
                      step={row.step}
                      onInput={(value) => updateLine(1, value)}
                    />
                  </>
                }
              >
                <NumberField
                  value={pairValue(row.key as PairKey, 0)}
                  step={row.step}
                  onInput={(value) => update(row.key as PairKey, 0, value)}
                />
                <NumberField
                  value={pairValue(row.key as PairKey, 1)}
                  step={row.step}
                  onInput={(value) => update(row.key as PairKey, 1, value)}
                />
              </Show>
            </div>
          )}
        </For>
        <p class="text-fs-0 text-fg-3">{t("debug.mask.columns")}</p>
      </div>
    </Dialog>
  );
}

/** 一个窄数字输入（开发工具，不需要 tokens 化的外观）。 */
function NumberField(props: {
  value: number;
  step: number;
  onInput: (value: number) => void;
}): JSX.Element {
  return (
    <input
      type="number"
      class="h-7 w-24 shrink-0 rounded-ui bg-surface-bar px-2 text-right text-fs-1 tabular-nums text-fg-1 outline-none"
      value={props.value}
      step={props.step}
      onInput={(event) => props.onInput(Number(event.currentTarget.value))}
    />
  );
}

/** 惰性加载要求 default 导出（`TitleBar` 里 `lazy(() => import(...))`）。 */
export default MaskDebugDialog;
