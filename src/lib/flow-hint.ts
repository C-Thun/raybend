/**
 * **flowbar 的功能提示区**（`design/main.md` §2.2；崔总 2026-11-01）。
 *
 * ```text
 * [工作流] [设置]        按住 Ctrl 键可拖动                    照片信息 [全屏]
 *                          ↑ 这块（居中、随窗口伸缩、离右边 info 留白）
 * ```
 *
 * # 它解决什么
 *
 * 界面上有些**只有做过才知道**的操作（按住 `Ctrl` 拖控制点、拖某处的边缘改尺寸…）。
 * 与其到处放「不带变量的固定说明文字」（`AGENTS.md` §2.20 禁止的那种），
 * 不如在**鼠标真的指到那个东西上时**，在 flowbar 这一条空着的带子上说一句 ——
 * 用一次就学会了，之后不再打扰任何一屏。
 *
 * # 口径
 *
 * 1. **一个位置，多条来源**：谁指到了就 `showFlowHint(owner, key)`，离开就
 *    `clearFlowHint(owner)`；`owner` 不同不会互相清掉（见下）。
 * 2. **文案由调用方翻译**：`lib` 不许 import `i18n`（`lint:arch`），所以这里只存**成品字符串**；
 *    调用方用 `t(key)` 现取 —— 提示是一次性的（指向即出现、离开即消失），
 *    不需要跟着语言切换实时重译。
 * 3. **同一时刻只显示一条**：后指到的覆盖前一条（鼠标只能指一个地方）。
 * 4. **粘住的来源不会遮住后来者**：`clear` 只清自己那条 —— 但覆盖之后，
 *    先前的 owner 离开时**不会**把新的那条清掉（归属对得上才清）。
 */

import { createSignal } from "solid-js";

export interface FlowHintState {
  /** 谁提的（同一 owner 重复 show 只替换文案） */
  owner: string;
  /** 已经翻译好的文案（`lib` 不许 import `i18n`，翻译在调用方做） */
  text: string;
}

const [hint, setHint] = createSignal<FlowHintState | null>(null);

/*
 * **开/关住在这一层**（崔总 2026-11-02）：设置里那个开关只改这里，
 * 每个调用点继续 `showFlowHint(...)` —— 谁都不许自己去看设置。
 *
 * 默认开；设备级偏好（`localStorage`，版本化 key，`AGENTS.md` §2.16）。
 * 关掉时 `flowHint()` 恒为 `null`，且"关"的那一刻会把正在显示的那条清掉。
 */
const STORAGE_KEY = "raybend.flow-hint.v1";

function readEnabled(): boolean {
  if (typeof window === "undefined") return true;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    // 只有明确的 "0" 才算关；没存过 / 读到垃圾都按默认（开）
    return raw !== "0";
  } catch {
    return true;
  }
}

const [enabled, setEnabledSignal] = createSignal(readEnabled());

/** 提示区是否启用（设置页的开关读它）。 */
export function flowHintsEnabled(): boolean {
  return enabled();
}

/** 打开 / 关闭提示区（关掉时清掉当前那条，并落盘）。 */
export function setFlowHintsEnabled(next: boolean): void {
  if (enabled() === next) return;
  setEnabledSignal(next);
  if (!next) setHint(null);
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
  } catch {
    // 写不进去不影响本次会话
  }
}

/** 当前要显示的提示文本（`null` = 这条带子上什么都不显示 / 提示区被关掉）。 */
export function flowHint(): string | null {
  if (!enabled()) return null;
  return hint()?.text ?? null;
}

/** 显示一条提示（后指到的覆盖前一条；同一 owner + 同一文案不重复写）。 */
export function showFlowHint(owner: string, text: string): void {
  if (!enabled()) return;
  const current = hint();
  if (current !== null && current.owner === owner && current.text === text) return;
  setHint({ owner, text });
}

/** 清掉自己那条；**不是自己那条就不动**（避免 A 离开时把 B 的提示吃掉）。 */
export function clearFlowHint(owner: string): void {
  const current = hint();
  if (current === null || current.owner !== owner) return;
  setHint(null);
}
