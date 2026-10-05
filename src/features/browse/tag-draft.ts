/** 标签草稿只描述人工操作；不复制 AI 结果，保存/撤销不会覆盖后台的新识别。 */
import type { PhotoTagState } from "../../api/organization.ts";
export interface TagEdit { name: string; manual: boolean | null; masked: boolean | null }
export const tagKey = (name: string): string => name.trim().normalize("NFC").toLowerCase();
export function stageTagEdit(current: readonly TagEdit[], next: TagEdit): TagEdit[] {
  const key = tagKey(next.name);
  const previous = current.find((item) => tagKey(item.name) === key);
  return [...current.filter((item) => tagKey(item.name) !== key), { name: next.name.trim(), manual: next.manual ?? previous?.manual ?? null, masked: next.manual === true ? false : next.masked ?? previous?.masked ?? null }];
}
export function tagDraftRows(state: PhotoTagState, edits: readonly TagEdit[]): { name: string; manual: boolean; ai: boolean; masked: boolean }[] {
  const names = new Map<string, string>();
  for (const name of [...state.manual, ...state.ai, ...state.masks, ...edits.map((item) => item.name)]) names.set(tagKey(name), name);
  const contains = (group: readonly string[], key: string) => group.some((name) => tagKey(name) === key);
  return [...names].map(([key, name]) => {
    const edit = edits.find((item) => tagKey(item.name) === key);
    return { name, manual: edit?.manual ?? contains(state.manual, key), ai: state.result?.valid === true && contains(state.ai, key), masked: edit?.masked ?? contains(state.masks, key) };
  }).filter((row) => row.manual || row.ai || row.masked);
}
