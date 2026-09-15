/**
 * 建库弹窗的**判定逻辑**（纯函数，`REPOSITORY.md` §2.4）。
 *
 * 弹窗要在用户按下确认**之前**就告诉他会发生什么 —— 三种结局：
 *
 * ```text
 * 目录里没有 catalog.db → 新建库（没有任何提示行）
 * 目录里已有 catalog.db → 登记已有库（不会覆盖；已登记过的话是「加一条路径」）
 * 路径有问题             → 拒绝确认（行内错误）
 * ```
 *
 * 判定行**不是禁用按钮的理由**（除了真的写不进去那种）：
 * 探测还没回来、或者只是个「已有库」的提示时，按钮该是能按的 ——
 * 让用户等一个网络/磁盘探测才允许点击，反而像卡住了。
 */

import type { RepositoryProbe } from "../../api/types.ts";

export type CreateHint =
  | "none"
  | "existing"
  | "existingRegistered"
  | "broken"
  | "notDirectory";

export interface CreateGate {
  /** 能不能按下「新建库」 */
  canSubmit: boolean;
  /** 判定行显示哪一种（`none` = 整行不出现） */
  hint: CreateHint;
  /** `hint === "existing"` 时的库名 */
  existingName: string | null;
  /** `hint === "broken"` 时的原因 */
  brokenMessage: string | null;
}

export interface CreateGateInput {
  /** 用户填的路径（可能带首尾空格） */
  path: string;
  /** 探测结果；还没回来时是 `null` */
  probe: RepositoryProbe | null;
}

/**
 * 由「路径 + 探测结果」推出按钮可用性与提示行。
 *
 * `probe === null`（还在探测）时**允许提交**：Rust 侧在建库前会再探一次，
 * 已经有库就用它的 ID、读不出来就拒绝 —— 重复探测在这里只是提速，不是安全边界。
 */
export function evaluateCreate(input: CreateGateInput): CreateGate {
  const path = input.path.trim();
  if (path === "") {
    return {
      canSubmit: false,
      hint: "none",
      existingName: null,
      brokenMessage: null,
    };
  }

  const probe = input.probe;
  if (probe === null || probe.kind === "empty") {
    return {
      canSubmit: true,
      hint: "none",
      existingName: null,
      brokenMessage: null,
    };
  }

  if (probe.kind === "existing") {
    return {
      canSubmit: true,
      hint: probe.registered ? "existingRegistered" : "existing",
      existingName: probe.name,
      brokenMessage: null,
    };
  }

  if (probe.kind === "notDirectory") {
    return {
      canSubmit: false,
      hint: "notDirectory",
      existingName: null,
      brokenMessage: null,
    };
  }

  return {
    canSubmit: false,
    hint: "broken",
    existingName: null,
    brokenMessage: probe.message,
  };
}

/**
 * 探测结果只对**它当时探的那个路径**有效。
 *
 * 用户改路径后，旧结果必须立刻作废 —— 否则「已有库」的提示会挂在一个
 * 完全不同的路径上，用户会以为它说的就是新路径。
 */
export function probeMatches(probePath: string | null, currentPath: string): boolean {
  if (probePath === null) return false;
  return probePath.trim() === currentPath.trim() && currentPath.trim() !== "";
}
