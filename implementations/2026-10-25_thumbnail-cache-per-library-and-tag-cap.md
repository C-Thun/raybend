# 小图缓存按库 + 标签 5000 硬上限

完成时间：2026-10-25 22:05:07（+0800）

## 1. 改动范围

按崔总 2026-10-25 的拍板，一次完成四件事：

1. **小图缓存跟着库走**：从全局 `<app data>/cache/_sources/thumbs.db` 改为
   `<库根>/cache/thumbs.db`（每库一份），`LibraryThumbs` 按 `repository_id`
   持有句柄，库释放/换位置时移除。
2. **384 档 AVIF q80 + 每库 2 GiB 维护阈值**：`PIPELINE_VERSION` 升 v17；
   384 签名 `avif-q80-grid-v17-embedded-v1`，1920/真身仍 q90。
3. **容量治理**：写时清同键旧签名（每资产最多 1 条 384）、GC 一次收敛、
   命中批量 touch（LRU）、旧全局 DB 升级时删除。
4. **标签系统硬上限 5000（全局）**：`ensure_tag` 满额拒绝新名、已有名照常；
   `ensure_names_capped` 让 AI/XMP 后台路径跳过新名；`tag_stats` /
   `tag_prune_unused`；标签弹窗计数/禁用/清理入口；筛选菜单加搜索（C）。

另修 `release-run.test.mjs` 的非 hermetic 问题（私钥转换只在 `--with-updater`
时执行；测试显式注入 `env: {}`）。

## 2. 关键决策与理由

- **2 GiB / 15 万张的容量反推**：单张预算 14.3 KB。实测 12 张真实照片（384 长边）：
  AVIF q80 12,753 B（15 万约 1.91 GB），AVIF q90 22,583 B（3.39 GB），
  JPEG q80 22,845 B（3.43 GB）。**JPEG 达不到目标**，所以 384 档用 AVIF q80；
  1920/真身不降画质。
- **thumbs.db 只存 384**：旧实现会把 `Screen`（1920，约 329 KB/张）写进小图库；
  15 万张就是几十 GB。现在 `ThumbsDb::put` 拒绝非 Grid，`view_image` 的
  Screen 全部路由到 `cache/full/`（未编辑 SOOC/RAW 也落 FullCache）。
- **清同键旧签名**：编辑确认/重置会换签名；旧行不删会一直攒到容量 GC。
  现在同一 `(cache_key, size_class)` 只留最新一条（`pinned` 除外）。
- **LRU**：`get` 是纯读（读连接 `query_only`），命中只把「用过」攒进内存，
  30 秒/200 条批量写，且一小时内重复命中只写一次；GC 才真正按最近使用淘汰。
- **标签范围不变**：标签仍在 `app.db`，全局跨库公用；5000 是**全局总条数**硬上限，
  不是查询上限。已用标签的删除/合并（跨库清 `asset_tags`）不做，只清
  `use_count = 0` 的未使用标签。
- **2 GiB 不是硬限制**：它是维护阈值/预期量级；不阻断导入/浏览。单库达到
  15 万张后库设置界面提示建议开新库。

## 3. 迁移与清理

- 旧全局 `_sources/thumbs.db`（含 `-wal`/`-shm`）在启动时一次性删除；
  各库按需重建（不迁移行：无法可靠归属到库）。
- `PIPELINE_VERSION` v16 → v17：1920 预览/真身缓存也按版本失效，按需重建；
  这是 q80 改动的正常代价。
- 标签：已有 >5000 条的历史数据**不删**，只禁止新建；界面显示 `total / 5000`，
  可用「清理未使用标签」降到上限以下。
- 库设置提示：`photosCount >= 150_000` 时显示「建议开新库」，不阻断。

## 4. 涉及文件

- 核心：`crates/raybend/src/thumbnail/cache.rs`（`LibraryThumbs`、put 闸门/清兄弟、
  GC 收敛、touch 队列、2 GiB）、`thumbnail/render.rs`（q80/v17）、
  `thumbnail/worker.rs`（命中 touch、测试）、`display/mod.rs`（Screen 不落库）、
  `store/tags.rs`（`MAX_TAGS`、`ensure_names_capped`、未使用清理）、`error.rs`（`TagLimit`）。
- Tauri：`browse.rs`（`LibraryThumbs` 会话 + `library_thumbs`）、`thumbs.rs`
  （全部调用点、Screen 路由、按库统计/清理、旧全局清理）、`issues.rs`、`develop.rs`、
  `export.rs`、`import.rs`、`repo.rs`、`photo_ai.rs`、`sidecar.rs`、`tags.rs`、`lib.rs`。
- 前端：`api/db.ts`、`api/browse.ts`、`api/types.ts`、`api/dto-contract.*`、
  `GlobalSettingsDialog.tsx`、`LibrarySettingsDialog.tsx`、`TagDialog.tsx`、
  `TagFilterMenu.tsx`、`Menu.tsx`、`tag-filter.ts` + 测试、`browse/store.ts`、i18n。
- 发布：`scripts/release.mjs`、`scripts/lib/release-run.test.mjs`。
- 文档：`AGENTS.md` §2.24/§6.4/§6.5/§11.5、`memory/FUNCTION-IMAGING.md`、
  `memory/ARCHITECTURE.md` §6.4、`memory/FUTURE.md` G26/G27、`memory/FINISHED.md`、
  `specs/thumbnail-tiers-and-import-memory.md`、新 `specs/thumbnail-cache-per-library.md`。

## 5. 验证

- `cargo test --workspace`：**1756 passed / 13 ignored / 0 failed**（1585 lib + 155 desktop lib + 16 其余；含新增的
  缓存隔离、put 闸门、清兄弟、GC 收敛、touch、标签上限、契约键表、SOOC 基准测试）。
- `pnpm typecheck && pnpm test`：**1324 passed / 0 failed**（含 `tag-filter` 5 条）。
- `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`：全过。
- `pnpm build`：通过。
- `pnpm test:release`：**12 passed / 0 failed**（在设置了 `TAURI_SIGNING_PRIVATE_KEY`
  的环境下也全绿；新增「unsigned 无 updater 不调 wslpath」回归）。
- `git diff --check`：干净。

## 6. 评审修订（同日）

本次 Review 直接修掉的问题（均已补测试 / 已重跑门禁）：

1. **SOOC 的 1920 用错基准**（严重）：`EditBase` 默认 `Raw`，未编辑照片的栈是空栈；
   `view_image` 的 `Screen` 分支直接把栈交给 `render_profile_cached`，于是
   JPG-only 资产报「raw 源文件不存在」、JPG+RAW 资产显示 RAW 解码而不是相机直出。
   新增 `profile_target()` 按 `IssueChoice` 强制基准（Sooc→Sooc、Raw→Raw、Latest 原样），
   并补 3 条单测。
2. **缓存打不开时直接报错**：`thumb_get` / `view_image` 里 `library_thumbs(...)?`
   会把「库已释放 / 权限 / 磁盘」抛给用户，整个网格变错误；改为**降级为不缓存**
   （记一条 warn，照常渲染）。降级时**不能**退回渲染源文件 —— 编辑过的照片会显示成
   未编辑的样子，所以把 `render_profile_thumb` 的像素那一半抽成
   `render_profile_pixels`，降级路径走 `render_profile_uncached`（带编辑栈、出不了像素给占位图）。
3. **维护占写线程太久**：`maybe_maintain` 一次最多删 5 万行，原来逐条自动提交
   （最坏 5 万次提交，占着写线程），改为**整次维护一笔事务**。
4. **清理未使用标签会删掉草稿里的标签**：草稿里刚建的标签 `use_count = 0`，
   清理后被删，保存时会用不存在的 id 落关联（照片只剩 `#id`）。
   清理后按新词典过滤 `adding` / `removing`，并加 `busy()` 守卫。
5. **需求「RAW 读头是伪需求」落到文档**：实施记录里那条「未修项」已删掉，
   不占记忆空间；旧记录加了一行指向本轮，避免 1 GiB / 全局 DB 描述误导。

## 7. 画稿同步（2026-10-25）

Pencil 连接正常，按本轮代码同步了三个位置（**`.pen` 的磁盘保存仍由崔总执行**：编辑在 Pencil 内存里）：

- `design/browse.pen` · `Components / 标签弹窗`（`o4M2cF`）：
  两态各加一行词典计数（`标签 1 284 / 5 000`，11px / `$fg-3`，位于输入框与搜索结果之间）；
  新增第三列 **`Modal / 标签（满额）`**（`DBqxz`）：计数行变 `标签 5 000 / 5 000 · 已达上限`、
  右侧 `清理未使用标签（96）`（主色文字按钮）、「创建」行禁用（`已达上限，不能新建` + `先清理`，整行 55% 不透明）。
- `design/browse.pen` · 新增 **`Components / 标签筛选菜单`**（`ZGQC3`）：
  顶部搜索框（`搜索标签…`）+ `共 1 284 个标签 · 输入以搜索` + 标签行（选中 = 主色底 + 右侧「已选」）。
- `design/main.pen` · 库设置（`Wg6x8` 在线 / `L4N9Dz` 浅色宽松）：在「计数与重建」块之后加一行
  `已超过 15 万张 · 建议开一个新库分担`（12px / `$fg-2`，不阻断）。
- 说明文档：`design/browse.md`（三态 + 新组件行）、`design/main.md`（15 万提示的界面出口；
  并记下「设置 → 文件页尚未进画稿」这一遗留）。
- 预览导出：`design/exports/tag-5000/{PGvh4,ZRANP,DBqxz,ZGQC3}.png`。
- 布局校验：`o4M2cF` 加宽到 1432、`DTAOc` 1392（三列 440 + gap 24），`Get` 复查无 clipped；
  菜单面板去掉了 1px 描边（描边只属于弹窗，菜单仍是无边线）。

## 8. 遗留 / 风险

- **真机 E2E 归崔总**：15 万库（或合成）、5000 标签、缓存占用/清理、q80 画质目视。
  `#[ignore]` 的 150k 容量基准（插入 15 万条 12.75 KB 合成数据）尚未在 Windows 真机跑。
- **标签清理信任 `use_count`**：只删 `use_count = 0`；该值由各库关联同步维护。
  若历史数据同步漂移，可能误删实际仍被引用的词（概率低；删除前未加二次确认弹窗）。
- **2 GiB 是 `sum(bytes)`**：SQLite 删行不收缩文件（页进 freelist），实际磁盘占用
  可能高于 2 GiB 且保持历史峰值。要严格收敛需要定期 `incremental_vacuum` 或
  `VACUUM`，代价是重写整库；本轮未做。
- **同一资产可能出现两条 384**：未编辑时按位图文件身份缓存、编辑后按 RAW 源身份缓存
  （键家族不同），只有容量 GC 才收。单条约 12.8 KB，影响可控；根治要改成按 asset_id 作键。
- **升级前的遗留队列**：`app.db` 里 `size = screen` 的旧缩略图任务现在会被
  `put` 拒绝（只存 384），任务记为失败；不影响新任务。
- **多库同时打开**：每库一份 thumbs.db（1 读连接 + 1 写线程），长期开很多库会累积句柄；
  未做「最多保留 N 个打开」的 LRU。
- **旧全局 DB 删除**：升级后旧版本若再次运行会重新生成它；当前版本不再读。
- **Pencil**：本轮已同步（见 §7）；`.pen` 仍未落盘 —— **需要崔总在 Pencil 里保存
  `design/browse.pen` 与 `design/main.pen`**，否则重开应用会丢这批改动。
- **XMP**：小图是派生数据，不影响 sidecar；标签上限不改 XMP 映射；清理未使用标签
  不碰照片关联。
