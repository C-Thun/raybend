2026-10-25 04:48:35 CST

# 发版前缺陷收口：桶/标签审查 3、5–15、发布说明、小图缓存 GC、XMP 自维护、老库尺寸、崩溃符号

> 同日晚些时候按库重构了小图缓存（384 改为 AVIF q80、每库 2 GiB、只存 384），并给标签加了 5000 硬上限：
> 见 `implementations/2026-10-25_thumbnail-cache-per-library-and-tag-cap.md`。本文件的 1 GiB / 全局 thumbs.db 描述是当时的实现。

## 背景与范围

崔总 2026-10-24 逐项拍板：

- `Future Release` 板上除 #2 / #3 / #10 外全部算完成（走完流程，使用中发现问题再改）。
- 缺陷第 1、2 项「处理掉，按你的方案」：桶/标签审查未修项、其他真实缺陷。
- 第 3 项：XMP 自维护的重点不是映射测试，而是 **AGENTS.md 里的项目图景 + 功能关联**，让以后加无损编辑参数时不可能漏 XMP；框架能兜底最好。
- 第 4 项：崩溃报告要在不传敏感信息的前提下尽量能准确定位。

本记录只覆盖本轮改动。工作树里另有并行会话的「弹窗叠放与边界线」与「国内镜像」改动（`Dialog.tsx` / `CommandPalette.tsx` / `dialog-stack.*` / `specs/website-cn-mirror.md` 等），**不是本轮产物**，未动。

## 一、桶 / 标签审查 3、5–15（`specs/photo-organization.md` §5.2 已补实现说明）

| 审查项 | 修法 |
| --- | --- |
| 3 死成员清不掉、计数不符 | `Bucket.memberCountsByRepository`（app.db 按库计数）+ 面板「当前挂载 / 全部」；新增 `organization_bucket_prune`：按在线库逐个核对 UID，删除死成员（**不写排除**）；照片区空态区分「空桶」与「成员都在未挂载库」 |
| 5 AI 重试要两步 | `ai::status::retry` 直接把父任务置回 `pending`（不再 `paused`），`photo_ai_task_action` 对 `retry` 也 `wake()`；单测期望同步改 |
| 6 规则预览吞离线库 | `BucketDialog` 预览区显示 `failedRepositories.length` 警告（`org.previewPartial`） |
| 7 从筛选建桶无默认库、无说明 | `rulesFromBrowseFilter` 新增 `repositoryIds` 参数，App 传当前库；`BucketDialog` 新增 `note`，筛选入口显示「导航范围不会带入」 |
| 8 AI 弹窗加载期闪「缺模型」 | `modelMessage()` 区分 loading / damaged / disabled / unpublished / missing；新增 `ai.loadingModel` |
| 9 加桶 toast 用请求数 | `dropIdsIntoBucket` 改用实际 `added`；0 条时提示「已经在相片桶里」 |
| 10 标签子目录不刷新 | `loadChildren()` 抽出；`refreshKey` 变化时重拉所有已展开标签的子目录 |
| 11 空桶与未挂载同文案 | 面板卡片显示 `0 / N`；照片区空态用 `org.bucketMembersOffline` |
| 12 目录标签移除无确认 | 改为 `ConfirmDialog`（`org.removeDirectoryTagConfirm`），确认后才 `setDirectoryTag(false)` |
| 13 点标签行不展开 | 行点击在选中后按 `tagHasChildren()` 展开；箭头仍只负责展开/折叠 |
| 14 词典限额静默截断 | 新增 `tag_list_all` 命令 / `tagListAll()`；标签弹窗与右栏词典改用全量，搜索仍走 `tag_list` 限额；面板超过 500 条显示「最多显示」提示，不再静默 |
| 15 补扫游标不清理 | `forget_repository` 同时删 `photo_organization_cursors`；测试补断言 |

第 4 项（桶改名入口）与「时间线全量物化」按崔总口径留在 after-release，本轮不动。

## 二、其他真实缺陷

### 1. 发布说明内部提示行

- `scripts/finalize-release.mjs`：删掉「发布前请按 docs/release.md……」内部提示行；新增 `docs/release-notes/<版本>.md` 约定，构建时嵌入「## 本版变更」，没有文件则放提交记录链接。
- `docs/release.md` §6.1 与新增附录 K 写清约定；`docs/release-notes/README.md` 说明写法。
- `scripts/lib/release-upload.test.mjs` 增加断言：发布说明不得含内部提示行、必须有本版变更区。
- 已发布的 v0.1.2 / v0.1.3 正文仍带旧行；改不改由崔总在发布侧决定（本轮不代改线上 Release）。

### 2. 库内小图缓存不回收（FUTURE G26）

- `ThumbsDb::maybe_maintain`：写缓存后调用，内部 6 小时节流；`maintenance` 先收**其它管线版本**的孤儿，再 `gc`（默认 1 GiB）。
- 孤儿判据（评审后定稿）：签名里的 `-v<数字>` 片段（`drop_other_pipeline_versions`）—— 当前家族全带 `-v<PIPELINE_VERSION>`，带编辑的动态签名也带 ⇒ 保住；老版本不含 ⇒ 收走；**不带版本片段的一律保留**（宁可漏收，不能误删）。原计划的「当前签名白名单 + `drop_stale_signatures`」在评审时弃用：漏登记一种缓存家族就会把活缓存每 6 小时删一遍。
- 接入点：`display::cached_image`、`thumbnail::worker`（`render_cached_with` 与 `run_pending`）、`thumbs.rs` 的 raw-original / prepare_source_images、`issues.rs` 定稿缩略图。
- 设置 → 文件页新增「小图缓存」：显示占用/条数、确认后清理（`thumb_sources_stats` / 新增 `thumb_sources_clear`）。
- 单测：只收其它版本（含旧 issue / 旧 grid，含带编辑的动态签名保留、无版本片段保留）、容量回收。

### 3. 老库 RAW 尺寸后台补齐（PLAN §5 #10）

- `store::sizes::assets_missing_display_sizes(conn, after, limit)`：只列缺列且在线文件所属资产，**按 `asset_id` 游标分页**（与 `ensure_file_sizes` 的写入口径一致：一列 NULL 也算缺，半残行能补）。游标而非「试过的 id 集合」：读不出尺寸的行会永远留在候选集里，只有游标能保证后面的资产也被试到。
- `BrowseState::maybe_backfill_sizes`：开库后后台线程每批 64 个资产、逐资产调用 `ensure_file_sizes`（单资产一次写事务），游标前进到表尾即结束；每步检查 `ensure_current()`；库释放/失败立即停并允许下次开库重试。
- 不扫全库、不持任务许可；只碰缺列行；逐资产提交，写入能穿插。

### 4. 崩溃符号与定位（崔总第 4 项）

- `Cargo.toml`：`strip = true` → `strip = "debuginfo"`；注释写明 MSVC 忽略 `-C strip`、符号由 PDB 提供。
- `release.mjs`：Windows 构建后把 `release/*.pdb` 归档到 `release-out/symbols/v<版本>/`（**不上传**）。
- `crates/raybend/build.rs` + `diagnostics::crash::AppInfo`：报告新增 `gitHash` / `buildTime`（缺失时省略），`report.txt` 也印 commit 与构建时间。
- `docs/release.md` 附录 K：说明「发布包不带 PDB、按版本归档、用 dump + PDB 离线定位」；PDB 是代码符号，不含用户照片/库内容。

## 三、XMP 自维护（崔总第 3 项）

- **AGENTS.md 新增 §13「项目图景与改动影响索引」**：功能 → 代码落点 → 会写什么 → 必须同步什么；§13.1 是「新增无损编辑参数」的 8 步固定清单（参数表 → 渲染 → 版本 → XMP 归类 → rb → preset → 文档 → 测试门）。
- §2.19 的 XMP 纪律指向 §13.1；`specs/xmp-sidecar.md` §5.1 补「维护闸门」段。
- **框架兜底**：`xmp/mapping.rs` 新增 `NOT_MAPPED`（带理由）与 `every_develop_param_is_classified_for_crs`；遍历 `develop::params::PARAMS`，每个参数必须且只能归入 `SCALARS` 或 `NOT_MAPPED`，新增参数不归类直接红。
- `rb:profileJson` 存完整 `DevelopStack`，新参数自动携带；只有 `crs:` 兼容层需要人工决定。
- `specs/xmp-sidecar.md` §10 增加「最小核验」四步（check:xmp + 编辑/标记/重置），完整 A–E 降为抽查。

## 四、验证

- `cargo test --workspace`：**1747 passed / 13 ignored / 0 failed**（core 1580、worker 7、desktop 151、主程序 2、crashpad 1、文档 2 等）。
- `pnpm typecheck && pnpm test`：1319 passed / 0 failed；`pnpm lint:colors` / `lint:arch` / `lint:i18n` 全过；`pnpm build` 通过。
- `node --test scripts/lib/release-upload.test.mjs scripts/lib/release-publish.test.mjs`：15 passed。
- 新增/改动的单测：桶计数与 prune、AI retry 状态、游标清理、尺寸候选、缓存维护（版本片段判据）/半残尺寸行、XMP 参数归类、发布说明断言、前端桶计数与筛选默认库。
- **未做**：GUI E2E / 真机（归崔总）；PDB 归档要到下一次 Windows 发布才实际产出；v0.1.2/v0.1.3 线上 Release 正文未改。

## 四之二、评审修订（2026-10-25 第三方评审）

1. **孤儿回收的失效方向改为「宁可多留」**：白名单改成签名里的 `-v<版本>` 片段判据（`drop_other_pipeline_versions`），删掉 `drop_stale_signatures` / `known_sig_prefixes`；`GLOB '*-v[0-9]*'` 避免 `-version` 这类单词误命中。
2. **半残尺寸行永远补不上**：候选查询用 `OR`、`ensure_file_sizes` 写前守卫用 `AND` —— 两者口径不一致；守卫改为 `OR` 并补单测（只填了一半的行能补齐）。另外补齐循环从「试过的 id 集合」改成**游标**：否则前 64 个读不出尺寸的文件就会让整库补齐提前认定完成。
3. **空态文案可能停在旧计数**：面板新增 `onBucketSnapshot`，桶列表刷新后把当前选中桶的新快照交给照片区；清理失效成员后不会再显示已过时的「库未挂载」。
4. **test 通道的 PDB 不再混进正式版目录**：符号归档目录按通道分流（`v<版本>` / `test-<时间戳>`）；否则 test 构建（版本号常等于当前版本）会把正式包的 PDB 覆盖成对不上的那份。
5. **文档口径校准**：`docs/release.md` 附录 K 写明「能到函数、不保证行号（未开 debug 信息）」；`crash.rs` 的旧注释（strip=true / 符号策略待定）同步更新；`AGENTS.md` §13.2 的缓存规则改为「新签名必须带 `-v<PIPELINE_VERSION>` 片段」。

评审确认的**未修**项（取舍或超范围）记在本轮评审报告里：1 GiB 上限在超大库可能抖动（反复淘汰重渲染）、`tag_list_all` 无上限负载、`drop_stale_signatures` 被删除属于公开 API 变更（`raybend` 仅 0.1.x 且该函数从未有调用点）。

## 五、遗留 / 归人类

1. 崔总四项真机验收不变：XMP 最小核验、bucket/tag + AI 实跑、回收站细测、全流程 + 导出。
2. 桶审查第 4 项（改名入口）与时间线全量物化仍在 after-release。
3. 下一次正式发布时确认 `release-out/symbols/v<版本>/` 有 PDB；若没有，说明 Windows 构建没有产出，需要补查。
4. 设置页缓存清理是设备级操作，删除后浏览时重建；不影响照片/编辑/导出。
