/**
 * XMP 写出开关（设备级设置 `xmp.write_sidecar`，`specs/xmp-write-toggle.md`）的判定。
 *
 * **默认开、只有明确的 `false` 才关**：缺失、读不到、读不懂的值都按开 ——
 * 向「有 sidecar」的一侧保守（它同时承担库内灾难恢复的半边，静默关闭的代价更大）。
 *
 * Rust 侧的权威判定是 `src-tauri/src/sidecar.rs::write_enabled_value`，两处必须同口径；
 * 这里没有复用 `getBooleanSetting`（它对读不懂的值给 false），那是另一种语义。
 */
export function xmpWriteEnabled(raw: string | null): boolean {
  return raw === null || raw.trim().toLowerCase() !== "false";
}
