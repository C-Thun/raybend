/**
 * 异步加载的四态（`ARCHITECTURE.md` §1 的纯逻辑层）。
 *
 * 为什么单独一个类型而不是各 feature 自己声明：它同时出现在
 * **feature 的 props**（视图要区分「还在读」与「读完了但没内容」）与
 * **workspaces 的 store**（谁在加载）里，而 feature 不能 import workspaces
 * （分层规则），所以这个类型必须住在大家都允许 import 的层。
 */

export type LoadStatus = "idle" | "loading" | "ready" | "error";

/** 还没开始读（初始态） */
export const IDLE: LoadStatus = "idle";

/** 是否处于「正在读」 */
export function isLoading(status: LoadStatus): boolean {
  return status === "loading";
}

/** 读完了（无论有没有内容） */
export function isSettled(status: LoadStatus): boolean {
  return status === "ready" || status === "error";
}
