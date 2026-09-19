/**
 * 数据库升级模块的对外出口（`ARCHITECTURE.md` §2：别的层只能从这里拿东西）。
 */

export { MigrationGate, type MigrationGateProps } from "./MigrationGate.tsx";
export {
  activeNotice,
  applyNotice,
  NO_MIGRATIONS,
  type MigrationMap,
} from "./notice.ts";
