//! 数据底座：本地 SQLite 库的打开、迁移、备份与写并发。
//!
//! 见 `AGENTS.md` §6.4（存储架构）、§7.1–§7.3，以及 `REPOSITORY.md`（库与导入规格）。
//!
//! 本模块只依赖 `rusqlite` 与标准库：**不认识 Tauri、不认识渲染**。
//! 路径与目录由调用方注入（`src-tauri` 从 Tauri 的 path API 取到后传进来）。
//!
//! 分层：
//!
//! | 模块 | 职责 |
//! | --- | --- |
//! | [`path_semantics`] | 路径的三种表示（原始 / NFC / 折叠），全库唯一的「路径相等」判据 |
//! | [`file_id`] | 文件身份 `(卷序列号, 文件 ID)`，用于稳定追踪重命名与移动 |
//! | [`pragma`] | SQLite 连接的统一 PRAGMA 设置（WAL、外键、忙等待） |
//! | [`migration`] | 迁移执行器：版本闸门 + 快照 + 逐条事务 + 完整性检查 |
//! | [`time`] | Unix 毫秒工具（库里时间统一用它） |
//! | [`pool`] | 读连接池（连接数有上限、借还靠 `Drop`、连接坏了就丢） |
//! | [`writer`] | 单写者 actor：所有写串行到一条连接上，退出时把队列跑干 |
//! | [`ids`] | 库 ID：可排序的定长 base62（时间 + 随机） |
//! | [`repository`] | 库身份、元信息、`app.db` 登记与在线/离线解析 |
//! | [`marking`] | 标记写入（评分/色标/喜欢/锁/标签）+ 撤销栈 |
//! | [`flags`] | 旗标：内存态、跨库跨目录的临时工作集（`BROWSE.md` §3.2） |
//! | [`delete`] | 删除：只到系统回收站，一级锁挡住（`BROWSE.md` §5.9） |
//! | [`query`] | 资产查询：范围 + 筛选 + 排序 + 分页（浏览网格的数据源） |
//! | [`fts`] | 全文检索索引的维护（派生索引，过期即在查询前重建） |
//! | [`tags`] | 标签：全局词典（`app.db`）+ 每库关联（`catalog.db`），`BROWSE.md` §7 |
//! | [`assets`] | 资产与文件记录的读写：差分计划的落库、配对、缺失标记 |
//! | [`backfill`] | 老库的元数据回填（`taken_at IS NULL` 的资产重读 EXIF） |
//! | [`recent`] | 最近导入过的目录（`app.db`）——「最近」列表的存取与裁剪 |
//! | [`volumes`] | 可选来源的枚列（Windows 驱动器 / Linux 挂载点）+ 来源类型判定 |
//! | [`db`] | 门面：`AppDb`（全局库）与 `CatalogDb`（每库），上层只用这一层 |
//! | [`location`] | 位置判定：本地 / 网络 / 云同步（放 catalog 的风险提示） |

pub mod assets;
pub mod backfill;
pub mod base_curve;
pub mod db;
pub mod delete;
pub mod develop;
pub mod file_id;
pub mod flags;
pub mod fts;
pub mod ids;
pub mod issues;
pub mod location;
pub mod luts;
pub mod marking;
pub mod migration;
pub mod path_semantics;
pub mod pool;
pub mod pragma;
pub mod query;
pub mod rebuild;
pub mod recent;
pub mod repository;
pub mod tags;
pub mod time;
pub mod volumes;
pub mod writer;
