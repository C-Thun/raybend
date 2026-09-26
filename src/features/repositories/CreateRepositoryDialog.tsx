/**
 * `CreateRepositoryDialog` —— 建库弹窗（画布 `Dialog / 新建库` `HFzJZ` / 错误态 `sccht`）。
 *
 * 判定逻辑（不做 I/O）在 `create-logic.ts`，有单测；这里只负责：
 *   * 收集输入（名称 / 库根目录）；
 *   * **探测**目录（防抖 250ms）→ 显示判定行；
 *   * 提交 → 交给调用方（建库成功后的选中与刷新由工作区做）。
 *
 * 两个环境的差异只有一处：「浏览…」没有原生选择器时（浏览器预览）
 * 只显示一句提示，用户仍可手打路径 —— 这也是 `specs/M1-5.md` §8 的退路。
 */

import { createEffect, createSignal, Show } from "solid-js";
import { IconAlertTriangle, IconInfoCircle } from "@tabler/icons-solidjs";
import * as db from "../../api/db.ts";
import { pickDirectory } from "../../api/dialog.ts";
import type { RepositoryProbe, RepositoryView } from "../../api/types.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { t } from "../../i18n/index.ts";
import { evaluateCreate, probeMatches } from "./create-logic.ts";

export interface CreateRepositoryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 建库成功（工作区负责刷新列表并选中它） */
  onCreated: (repository: RepositoryView) => void;
}

/** 探测防抖：用户还在打字时不要每敲一个字符就查一次盘 */
const PROBE_DEBOUNCE_MS = 250;

export function CreateRepositoryDialog(props: CreateRepositoryDialogProps) {
  const [name, setName] = createSignal("");
  const [path, setPath] = createSignal("");
  const [probe, setProbe] = createSignal<RepositoryProbe | null>(null);
  const [probedPath, setProbedPath] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [pickerUnavailable, setPickerUnavailable] = createSignal(false);

  // 每次打开都从干净状态开始（上次的路径/错误不该留在下一次）
  createEffect(() => {
    if (props.open) {
      setName("");
      setPath("");
      setProbe(null);
      setProbedPath(null);
      setError(null);
      setSubmitting(false);
      setPickerUnavailable(false);
    }
  });

  // 路径变了就重新探测（防抖）；探测失败不显示成「错误」，只是没有提示行
  createEffect(() => {
    const current = path().trim();
    if (current === "") {
      setProbe(null);
      setProbedPath(null);
      return;
    }
    const timer = setTimeout(() => {
      void db
        .probeRepository(current)
        .then((result) => {
          setProbe(result);
          setProbedPath(current);
        })
        .catch(() => {
          setProbe(null);
          setProbedPath(null);
        });
    }, PROBE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  const gate = () =>
    evaluateCreate({
      path: path(),
      // 旧路径的探测结果不能用来判断新路径
      probe: probeMatches(probedPath(), path()) ? probe() : null,
    });

  async function browse(): Promise<void> {
    const picked = await pickDirectory({ title: t("repo.create.path") });
    if (picked === null) {
      // 没有选择器（浏览器预览）或用户取消：给一句提示，不打断输入
      if (path().trim() === "") setPickerUnavailable(true);
      return;
    }
    setPickerUnavailable(false);
    setPath(picked);
  }

  async function submit(): Promise<void> {
    if (!gate().canSubmit || submitting()) return;
    setSubmitting(true);
    setError(null);
    try {
      const created = await db.createRepository(
        path().trim(),
        name().trim() === "" ? undefined : name().trim(),
      );
      props.onCreated(created);
      props.onOpenChange(false);
    } catch (caught) {
      setError(
        t("repo.create.failed", {
          message: caught instanceof Error ? caught.message : String(caught),
        }),
      );
    } finally {
      setSubmitting(false);
    }
  }

  const hint = () => {
    const current = gate();
    switch (current.hint) {
      case "existing":
        return t("repo.create.hint_existing", {
          name: current.existingName ?? "",
        });
      case "existingRegistered":
        return t("repo.create.hint_existing_registered", {
          name: current.existingName ?? "",
        });
      case "broken":
        return t("repo.create.hint_broken", {
          message: current.brokenMessage ?? "",
        });
      case "notDirectory":
        return t("repo.create.hint_not_directory");
      default:
        return null;
    }
  };

  const hintTone = () =>
    gate().hint === "broken" || gate().hint === "notDirectory"
      ? "danger"
      : "muted";

  return (
    <Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("repo.create")}
      footer={
        <>
          <Button
            variant="secondary"
            onClick={() => props.onOpenChange(false)}
            disabled={submitting()}
          >
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            onClick={() => void submit()}
            disabled={!gate().canSubmit}
            loading={submitting()}
          >
            {t("repo.create.submit")}
          </Button>
        </>
      }
    >
      <div class="flex flex-col gap-3">
        <p class="text-fs-1 text-fg-2">{t("repo.create.intro")}</p>

        <label class="flex flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("repo.create.name")}</span>
          <Input
            value={name()}
            placeholder={t("repo.create.name")}
            onInput={(event) => setName(event.currentTarget.value)}
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("repo.create.path")}</span>
          <div class="flex items-center gap-2">
            <Input
              value={path()}
              class="min-w-0 flex-1"
              // 示例路径走 i18n：JSX 属性字符串**不处理反斜杠转义**，
              // 直接写 "D:\\Photos\\Library" 会原样显示出两个反斜杠（2026-09-16 验收反馈）
              placeholder={t("repo.create.path_hint")}
              onInput={(event) => setPath(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submit();
              }}
            />
            <Button variant="secondary" onClick={() => void browse()}>
              {t("repo.create.browse")}
            </Button>
          </div>
        </label>

        <Show when={pickerUnavailable()}>
          <p class="text-fs-0 text-fg-3">{t("repo.create.browse_unavailable")}</p>
        </Show>

        {/* 判定行：没有要提醒的就整行不出现（画布上就是这个口径） */}
        <Show when={hint()}>
          {(text) => (
            <div
              class={[
                "flex items-center gap-2 rounded-ui bg-surface-track px-2 py-1",
                hintTone() === "danger" ? "text-danger" : "text-fg-2",
              ].join(" ")}
            >
              <Show
                when={hintTone() === "danger"}
                fallback={<IconInfoCircle size={14} aria-hidden="true" />}
              >
                <IconAlertTriangle size={14} aria-hidden="true" />
              </Show>
              <span class="min-w-0 flex-1 text-fs-1">{text()}</span>
            </div>
          )}
        </Show>

        <Show when={error()}>
          {(message) => (
            <p class="text-fs-1 text-danger">{message()}</p>
          )}
        </Show>
      </div>
    </Dialog>
  );
}
