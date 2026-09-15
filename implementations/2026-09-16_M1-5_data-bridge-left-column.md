# M1-5 A 批：数据桥 + 导入工作区左列

完成时间：2026-09-16 04:31:28 CST

## 本次改动的范围

把「最近目录 / 卷 / 目录 / 照片 / 缩略图 / 库」这几件事从 Rust 打通到前端，并把导入工作区的**左列**（最近 / 来源 / 已选目录）做出来。

## 涉及文件

#### 核心层（`crates/raybend`）

| 文件 | 内容 |
| --- | --- |
| `src/store/migrations/app_0003_recent_dirs.sql` | `recent_dirs` 表（`path` 唯一索引 + `path_fold` 大小写折叠列，见 `AGENTS.md` §7.3） |
| `src/store/recent.rs` | 最近目录：记一条 / 列 50 条 / 移除；**按折叠路径去重** |
| `src/store/volumes.rs` | 卷枚列：Windows `GetDriveTypeW`、Linux `/proc/mounts`；归类 local/removable/optical/network/cloud |
| `src/media/source.rs`、`src/media/kind.rs` | 目录枚列与照片扫描的收口（扫描事件复用 M1-3 的 `media::scan`） |
| `src/thumbnail/cache.rs`、`worker.rs` | 来源侧缩略图缓存（`<app data>/cache/_sources/thumbs.db`，缓存键含 FileId） |

#### 外壳（`src-tauri`）

`src/source.rs`、`src/repo.rs`、`src/thumbs.rs`、`src/db.rs`、`src/lib.rs`、`src/contract.rs` —— 命令一律 `async` + `spawn_blocking`（同步命令跑主线程会卡窗口，这条是硬纪律）。

#### 前端

`src/api/{db.ts,types.ts,dto-contract.json,dto-contract.test.ts}`、`src/features/{recent,source-tree,selected-dirs,empty-state}`、`src/workspaces/import/{LeftColumn.tsx,store.ts}`、`src/lib/{tree,shortpath,checked-dir,load-status}.ts`、`src/i18n/*`。

## 关键决策与理由

1. **类型桥手写 TS 镜像 + 契约夹具**：`src/api/dto-contract.json` 被 **Rust 与 TS 两侧的测试同时读**（`src-tauri/src/contract.rs` 与 `src/api/dto-contract.test.ts`）。
   这不是为了好看：它当场抓到一个真 bug —— `CacheStats` 少了 `#[serde(rename_all = "camelCase")]`，前端拿到的字段名会全是 snake_case，而**两边单独跑测试都不会报**。
   没上 tauri-specta：见 `FUTURE.md` G17（有明确触发条件再评估）。
2. **`thumb_get` 返回原始字节**（`tauri::ipc::Response`），不走 base64/JSON 数组：
   走 JSON 会把每张缩略图放大 ~1.4 倍并让主线程解析字符串，M1-6 的「瞬间出图」会直接死在 IPC 上。
   并发用信号量限到 4，前端侧另有 LRU + 请求代次（generation token）防串图。
3. **最近目录写「勾选目录」的时机**，不是「浏览过」：浏览是探索，勾选才是有意图（`BROWSE.md`）。
4. **EXIF 分两步**：`source_scan` 先出列表（`taken_at` 用文件名 → mtime 兜底，毫秒级），
   用户真按「按时间」时才 `source_times` 并行读真 EXIF（4 线程）。
   理由：实测 RW2 读 EXIF 要**整文件读**（9p 上 224ms/张），不能压在一次扫描里 —— 导入后时间在库里，这条只影响导入前的预览。
5. **`list_dirs` 去掉了冗余 `stat`**：实测 592ms → 19ms（9p 共享目录下 `stat` 代价极高）。

## 验证方式

- `cargo test -p raybend`（含 `store::recent` 14 项：折叠去重 / 裁到 50 / 移除 / 中文与超长路径）
- `cargo test -p raybend-desktop`（契约测试：夹具与 Rust 结构体互相校验）
- `pnpm test / typecheck / lint:arch / lint:colors / build`
- `pnpm smoke:ui`：三列几何（58/104/70px = 25/45/30%）

## 遗留问题

- 齿轮（库设置）不在本单元；属 M1-6。
- `src-tauri/tauri.linux.conf.json` 的开发期例外仍在（`FUTURE.md` G11）。
