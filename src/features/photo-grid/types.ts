/**
 * 照片网格用到的类型。
 *
 * 传输形状（`SourceItem` 等）住在 `src/api/types.ts` —— 那里是 IPC 契约的镜像，
 * 这里只是把它和「网格需要的最小设置接口」一起转出去，好让本模块内部
 * 不必到处 import 三个不同路径。
 */

export type {
  SourceItem,
  SourceScan,
  ThumbSize,
  TimeEntry,
} from "../../api/types.ts";

/**
 * 设置读写的最小接口。
 *
 * 单独声明而不是直接吃 `src/api/db.ts` 的全部导出：这个 feature 只需要
 * 「读一个值、写一个值」，将来换存储（比如全放 localStorage）时不用动 store。
 */
export interface SettingsApi {
  getSetting: (key: string) => Promise<string | null>;
  setSetting: (key: string, value: string) => Promise<void>;
}
