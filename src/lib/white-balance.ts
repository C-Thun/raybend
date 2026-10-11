/**
 * 白平衡的 **mired 偏移**换算（`specs/editor-presets.md` §3 v3）。
 *
 * 预设里不再存绝对色温 K，而存「相对这张照片 as-shot 的偏移」——
 * 套到另一张照片得到的是**同一种冷暖倾向**，而不是同一个绝对温度
 * （崔总 2026-10-10 定，`specs/editor-session-pipeline.md` §6.4）。
 *
 * ```text
 * mired(K) = 1_000_000 / K
 * Δmired   = mired(asShot) − mired(target)
 * target   = 1_000_000 / (mired(asShot) − Δmired)，clamp 到 2500..10000
 * ```
 *
 * ⚠️ **符号**：Δ = 0 = 跟随拍摄色温；Δ **负** = 更暖（target K 更低），
 * Δ **正** = 更冷（target K 更高）。规格原文把「正 = 更暖」写反了，
 * 公式本体以本文件与 `specs/editor-presets.md` §3 的修正版为准。
 *
 * 纯函数、无依赖：换算只有这一份（前端）。Rust 侧不参与预设换算，不写第二份。
 */

/** 色温拉杆的范围（与 `params.rs` 的 `ParamSpec` 同口径）。 */
export const WB_KELVIN_MIN = 2500;
export const WB_KELVIN_MAX = 10000;

/** 1e6 / K —— mired（微倒度）。 */
export function mired(kelvin: number): number {
  return 1_000_000 / kelvin;
}

function clampKelvin(kelvin: number): number {
  return Math.min(WB_KELVIN_MAX, Math.max(WB_KELVIN_MIN, kelvin));
}

/**
 * 目标色温 → 相对 as-shot 的 Δmired（存进预设的那个值）。
 *
 * `asShotK === null`（照片没给拍摄色温）→ `0`（跟随拍摄色温）：
 * 没有基线就没有「相对谁」，存一个绝对值等于撒谎（旧预设污染的来源）。
 * 非有限的输入同样按 `0` 处理（坏输入不许进 payload）。
 */
export function kelvinToWbMired(targetK: number, asShotK: number | null): number {
  if (asShotK === null || !Number.isFinite(asShotK) || asShotK <= 0) return 0;
  if (!Number.isFinite(targetK) || targetK <= 0) return 0;
  return mired(clampKelvin(asShotK)) - mired(clampKelvin(targetK));
}

/**
 * Δmired → 目标色温（套用预设时算的那个值），结果 clamp 到 2500..10000。
 *
 * 极端偏移（分母 ≤ 0 = 数学上超出 0K）截到**最暖端 2500K** ——
 * 那是「更暖」方向的极限，与 Δ 的单调方向一致（越大的 Δ 越冷，
 * 越过极点之后不再有意义，只能截断）。
 */
export function wbMiredToKelvin(asShotK: number, deltaMired: number): number {
  const asShot = clampKelvin(Number.isFinite(asShotK) && asShotK > 0 ? asShotK : 6250);
  const delta = Number.isFinite(deltaMired) ? deltaMired : 0;
  const denominator = mired(asShot) - delta;
  if (denominator <= 0) return WB_KELVIN_MIN;
  return clampKelvin(1_000_000 / denominator);
}
