/**
 * 右栏文字字段的本地回填：**与后端 `marking::set_text` 同口径**的规整 + 就地改条目。
 *
 * 为什么要有这份（崔总 2026-11-03 实测的 1 秒延迟）：右栏显示的作者/描述来自
 * `entries()`（网格那一份 `AssetItem`），而 `store.mark()` 只回填标记四项。
 * 文字写入后不再依赖「sidecar 写盘 → 文件监听 → 整目录重扫」这条**偶然**的旁路，
 * 而是把后端刚写下去的值直接贴回条目：右栏立刻更新，XMP 写到哪一步都不影响界面。
 *
 * 口径必须与 Rust 一致（`crates/raybend/src/store/marking.rs::set_text`）：
 * 先 `trim`，空串 → `NULL`（这里是 `null`）。
 */

import type { AssetItem, EditableTextField } from "../../api/types.ts";
import type { TextFieldKey } from "../../lib/text-history.ts";

/** 编译期对齐：`lib` 不依赖 `api`，但不能允许两边字段集合悄悄漂开。 */
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
const _fieldsMatchApi: Same<EditableTextField, TextFieldKey> = true;
void _fieldsMatchApi;

/** 与后端同口径：去首尾空白；空串表示**清空该字段**（落库为 NULL）。 */
export function normalizeTextValue(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/**
 * 把一次文字写入就地贴回到已加载条目里（**只动命中的 id**，别的对象保持原引用）。
 *
 * `ids` 是后端真的写成功的那些（调用方已剔掉 `skippedLocked`）。
 */
export function patchTextFields(
  items: readonly (AssetItem | null)[],
  ids: ReadonlySet<number>,
  field: EditableTextField,
  value: string,
): (AssetItem | null)[] {
  if (ids.size === 0) return items as (AssetItem | null)[];
  const next = normalizeTextValue(value);
  let changed = false;
  const patched = items.map((item) => {
    if (item === null || !ids.has(item.id) || item[field] === next) return item;
    changed = true;
    return { ...item, [field]: next };
  });
  return changed ? patched : (items as (AssetItem | null)[]);
}

/**
 * 「按 GPS 填入地点」的就地回填：**只填空字段**（与后端 `marking::fill_geo` 同口径，
 * 人已经写过的值不覆盖）。三个字段一次改完，与那一步撤销对应。
 */
export function fillGeoFields(
  items: readonly (AssetItem | null)[],
  ids: ReadonlySet<number>,
  values: { country?: string | null; provinceState?: string | null; city?: string | null },
): (AssetItem | null)[] {
  if (ids.size === 0) return items as (AssetItem | null)[];
  const entries = (
    [
      ["country", values.country],
      ["provinceState", values.provinceState],
      ["city", values.city],
    ] as const
  )
    .map(([field, value]) => [field, value?.trim() ?? ""] as const)
    .filter(([, value]) => value !== "");
  if (entries.length === 0) return items as (AssetItem | null)[];
  const read = (item: AssetItem, field: (typeof entries)[number][0]): string | null =>
    field === "country"
      ? item.country
      : field === "provinceState"
        ? item.provinceState
        : item.city;
  let changed = false;
  const patched = items.map((item) => {
    if (item === null || !ids.has(item.id)) return item;
    const patch: { country?: string; provinceState?: string; city?: string } = {};
    for (const [field, value] of entries) {
      const current = read(item, field);
      if (current !== null && current.trim() !== "") continue; // 人写过的值不覆盖
      patch[field] = value;
    }
    if (Object.keys(patch).length === 0) return item;
    changed = true;
    return { ...item, ...patch };
  });
  return changed ? patched : (items as (AssetItem | null)[]);
}
