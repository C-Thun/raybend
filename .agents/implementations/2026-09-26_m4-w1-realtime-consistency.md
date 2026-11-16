# M4-W1 库与磁盘实时一致性
完成时间：2026-09-26 15:39:06 CST

## 范围与批准依据

崔总于 2026-09-26 确认 M4 重排路线并指示「开干」。本次完整实施 M4-W1，
覆盖安全局部同步、原生有界监听、目录计数/元数据/FTS 与派生图联动。
M4-W0 的导出画稿修订、W2–W7 导出与外部编辑功能尚未实施；没有把 M4 整体标为完成。
未提交、推送、打 tag 或生成发布包。Windows 产物为已有 debug 构建流程。

当前工作区包含另一轮 M3 收尾的未提交内容。本次沿用其 app v7 / catalog v11、
SOOC/latest/命名 issue、LUT 与原生显影入口，没有新增 schema，也没有重写 issue 模型。

## 实现与关键决定

1. **范围扫描与安全落库**：一般化现有 rebuild/scan/diff/assets 链，局部只扫目录及其实际大小写的 `_RAW`；
   全库重建共用这一套。扫描失败、取消、全库深度截断、photos 根离线/不可读均报错，
   不将未完整覆盖的内容当作缺失。库 ID、库内路径、规范化后的边界与符号链接均校验。
   事务内重读现状；只有仍与扫盘前快照一致的行可以置缺失，防止并发注册被误删。
   落库前再次检查文件事实，缺失落库时再次读文件系统；读失败回滚。
2. **文件身份与移动**：扫描携带 FileId/创建时间。可靠唯一身份且旧路径确已不存在时认移动，
   保留 asset、评级/标签与不可变 issue；身份未知或重复（硬链接）不强行猜配。
   同路径替换更新身份，恢复文件撤销缺失；名字交换/旧名复用先在事务内暂存路径再落最终路径。
   RAW 先后登记配对、RAW 大小写与中文目录的边界共用现有规范化函数。
3. **跨库补偿**：catalog 的 `repository_meta` 保存版本化 `catalog-sync.pending.v1`，
   与资产变化同事务写入变化 ID/路径、计数事实与全量重置标记。
   持久缓存失效、app.db 计数提交成功才确认；失败或重启保留待补偿事实。
   计数来自同一次成功扫描，不再二次读取时吞错；手动重建复用 BrowseState 的单一 CatalogDb 写者。
4. **原生监听**：采用 notify 8.2.0，关闭默认 macOS feature；Windows/Linux 原生实现。
   最多保留 32 个访问范围，监听目录、实际 `_RAW`、父目录；全部非递归。
   photos 的父目录捕获 photos 整体替换，目录身份变化重新安装 watcher。
   DB/cache/隐藏/临时/XMP 事件在入队前排除；有界通道、200ms 去抖、连续事件最多等待 1s，
   溢出转为重扫有界活跃范围。换库/退出释放监听，不因监听而取消进入/展开/回焦点读盘。
5. **统一变更链**：扩展已有 `repository_sync_dir` 的可选范围批次，浏览取数先等待同步，
   再取清单/时间线/分面。App 合并 native dirty/focus 请求并单飞；目录树复用既有展开重读。
   成功后一个 catalog change 事件刷新计数和图片队列；失败保留既有清单/树，错误可见。
   元数据仅刷新受影响资产，FTS 同步；并发文件事实变更时拒绝提交过期 EXIF。
6. **派生图与显影**：源签名由 FileId、文件大小、精确修改时间（纳秒）生成，
   复用同一缩略图 key helper 与 FullCache 文件命名函数。源变化删除相关小图和全部派生 AVIF，
   保留 profile/issue。latest/命名 issue 与 issue 小图签名均包含源版本，迟到任务写旧版本不能被新源命中。
   编辑线程同时比较 RAW 与 SOOC 的签名；源未变复用线性图及降噪结果，变化时重解码并刷新参照。
   查看器自动重取受影响的比较画幅，保留无关缓存/在途请求，用请求代号阻止旧图覆盖新图。
   原生事件传库根与相对路径，API 复用 joinPath 生成与网格相同的 Windows/UNC 缓存键。

## 涉及文件

- 核心：`store/{rebuild,assets,backfill}.rs`、`media/{diff,source,watch,mod}.rs`。
- 派生图：`thumbnail/worker.rs`、`display/{full_cache,mod}.rs`。
- 外壳：`src-tauri/src/{browse,repo,thumbs,issues,editor}.rs`。
- 前端：`src/App.tsx`、`src/api/db.ts`、`features/browse/{store,BrowsePanels}`、
  `lib/catalog-refresh`、`components/ui/viewer/store`、browse/editor workspace 及对应单测。
- 依赖与规格：`crates/raybend/Cargo.toml`、`Cargo.lock`、`THIRD-PARTY-NOTICES.md`、
  `BROWSE.md`、`IMAGING.md`、`FUTURE.md`、`PLAN.md`、`plans/M4-W1.md`。
- `design/export.md` 同步已批准行为口径与旧文案覆盖关系；本次未改 export.pen。

## 命令与热键

本波是既有目录刷新/手动重建行为的一般化，继续使用现有命令；不新增独立命令或硬编码热键。
导出新增动作与键位属于 W2/W5，须届时在统一注册表同波接入。

## 已验证：单元与冒烟

- `pnpm typecheck`：通过。
- `pnpm test`：966 项通过，0 失败，约 1.04s。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：全部通过。
- `cargo test -p raybend --lib`：1125 项通过，4 项既有忽略，0 失败，9.09s。
- `cargo test -p raybend-desktop --lib`：88 项通过，1 项既有忽略，0 失败，0.02s。
- `cargo check --workspace`：通过。
- `pnpm smoke:ui`：最终报告 problems 为空；这只证明开发预览外壳冒烟，不能验证真实库或原生显影。
- `pnpm debug:win`：通过；最终 exe 为 2026-09-26 15:37:19 CST，dist 为 15:36:37 CST，4 个资源全部命中，worker 包含 raybend-worker-proto-v3。脚本先 `pnpm build`，再构建 desktop 与 RAW worker，
  `pnpm check:win` 校验当前 dist 资源/时间与 worker 协议内容。没有启动 exe 或执行 GUI E2E。
- `git diff --check`：通过。

重点单测覆盖局部空目录不影响其他范围、RAW 配对/大小写、中文/Unicode、
同卷跨目录移动保留 issue、名字交换、同路径替换、缺失恢复、非法路径/离线/不完整扫描、
并发登记/元数据竞争、硬链接歧义、符号链接越界、补偿重启重放、原生事件/目录替换/事件风暴、
Windows/UNC 路径及比较图迟到请求。没有据此声称 Windows 文件监听真机行为已验收。

日志位于 `/tmp/raybend-m4-w1-{tests,rust-tests,tauri-tests,cargo-check,smoke,win-build}.log`。
最终 Windows 调试程序：`/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`；
配套 worker 位于同目录。

## 工具问题及处理

默认 sandbox 因 WSLg socket mount 冲突不能启动，确定路径读写/构建使用自动审核批准的
require_escalated；未修改 sandbox 配置。当前工具集没有 plannotator/todos，已在对话说明，
按本次明确授权执行完整 W1，以 `plans/M4-W1.md` 单一记录，不冒称经过工具评审。

早期全局 cargo fmt 涉及既有 dirty 工作区；基于 token 近似判断的批量恢复被自动审核拒绝，
已说明并停止该方案。后改为只读证明 `当前文件 == rustfmt(HEAD)`，核对双 SHA 后获准恢复
53 个纯格式文件；有语义差异的 M3 文件不回滚，可能保留其中不影响语义的格式变化。
后续只格式化本波明确修改的 Rust 文件，未再全局格式化。

## 限制与真实环境验收

源签名是文件系统事实摘要，不是完整像素/文件内容哈希。若外部工具刻意同时保留身份、
大小和精确修改时间，需要显式重建派生缓存；本波不增加读盘哈希负担。
监听覆盖有界活跃范围；未访问的深层目录不会因 watcher 而全库扫描，进入/展开/回焦点重新确认。
本波没有增加色彩管理、导出、XMP 写回或外部编辑器功能。

按 AGENTS.md §2.8，以下需要崔总在真实 Windows 环境确认：

- 程序外增加/删除 JPG 与 RAW 配对、中文/大小写改名及跨目录移动；清单和两个计数自动更新，评级/标签/issue 保留。
- 覆盖当前照片/比较图、替换 RAW 或 SOOC；小图、预览、原生显影与参照都更新，缩放/选择无意外丢失。
- 目录整体替换、库暂时离线、权限失败及恢复；失败可见，不误标全库缺失，恢复后数据正确。
- 空闲时监听、重新进入/展开目录、切库与回焦点；真实大库/DPI/性能体感与视觉/颜色正确性。

W1 开发与冒烟完成，真实环境验收仍待确认。下一工作单元进入 W0 的导出画稿修订和设计确认，
之后再实施 W2；不把尚未确认的 Pencil 修订跳过去。
