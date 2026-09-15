/**
 * `recent` 模块的唯一对外出口（`ARCHITECTURE.md` §2）。
 *
 * 这一层只导视图与它的 props：**数据与写入时机不在这里** ——
 * 「最近」的读写属于导入工作区的共享状态（`workspaces/import/store.ts`），
 * 因为它与「勾选目录」是同一个动作的两半。
 */

export { RecentList } from "./RecentList.tsx";
export type { RecentListProps } from "./RecentList.tsx";
