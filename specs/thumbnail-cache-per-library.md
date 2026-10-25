# 小图缓存按库（384 / AVIF q80 / 每库 2 GiB）

> 2026-10-25 崔总拍板。取代「全局 `_sources/thumbs.db`」的旧布局。
> 唯一事实源仍是 `memory/FUNCTION-IMAGING.md`（格式与档位）与 `AGENTS.md` §6.5（缓存红线）；
> 本规格记录**本次改动**的位置、容量、回收、迁移与验收。

## 0. 目标与基础保障指标

- **基础保障指标**：单库 **15 万张**（RAW+JPG 算一张）下，384 小图缓存 **≤2 GiB**，
  浏览不因容量抖动；超出按 frecency 淘汰、按需重建。
- 2 GiB 是**维护阈值/预期量级**，不是硬限制：不阻断导入、不阻断浏览；
  单库达到 15 万张后，库设置界面提示「建议开新库」（不阻断）。
- 384 档用 **AVIF q80 / 4:4:4**；1920 载入占位与真身仍 **AVIF q90**。
  实测 12 张真实照片：q90 22.6 KB、q80 12.75 KB；15 万 × 12.75 KB ≈ 1.8 GiB。
  JPEG q80 是 22.8 KB（15 万要 3.4 GB），达不到目标，所以不是 JPEG。

## 1. 位置与生命周期

```text
<库根>/
├── catalog.db
├── photos/
├── recycle/
└── cache/
    ├── thumbs.db      # 384 小图（每库一份，SQLite BLOB）
    └── full/          # 1920 载入占位 / 真身 / 命名 issue（FullCache）
```

- `LibraryThumbs`（`raybend::thumbnail::cache`）按 `repository_id` 持有 `Arc<ThumbsDb>`；
  `BrowseState` 持有这个映射。
- 懒打开：`<库根>/cache/thumbs.db`；库释放 / 换位置 / 忘记库时 `release`；
  后台任务手里的 `Arc` 自然活到任务结束。
- 打开失败（权限 / 磁盘）**只降级为不缓存**，照常出图并记日志。
- 删除 `cache/thumbs.db` 只降级、可从原图重建（红线不变）。
- 旧的全局 `<app data>/cache/_sources/thumbs.db` 不再有读者：升级后一次性删除
  （含 `-wal`/`-shm`），各库按需重建。不迁移行（无法可靠归属到库）。

## 2. thumbs.db 只存 384

- `ThumbsDb::put` 拒绝 `Screen` / `Full`；`display::cached_image` 对库内非 `Grid`
  不读也不写 thumbs.db。
- 1920 一律走 `cache/full/`（`FullCache`）：
  - 编辑过的照片：`render_latest_cached` / `render_profile_cached`；
  - 未编辑 SOOC / RAW 的 Screen：`view_image` 也路由到 `render_profile_cached`
    （变体 `sooc` / `raw`），不再把 329 KB/张的 screen 图混进 2 GiB 预算。
- 为什么必须：旧库里存在 `avif-q90-screen` 行（每条约 329 KB）；15 万张 screen
  就是几十 GB，2 GiB 目标直接失效。

## 3. 容量、回收与签名

| 项 | 规则 |
| --- | --- |
| 上限 | 每库 `DEFAULT_MAX_BYTES = 2 GiB`，按 `sum(bytes)` 算（SQLite 文件实际多 5–10%） |
| 质量 | 384 签名 `avif-q80-grid-v17-embedded-v1`；`PIPELINE_VERSION = 17`；1920/真身仍 q90 |
| 写时清兄弟 | `put` 后删同一 `(cache_key, size_class)` 的其它签名（`pinned` 除外）→ 每资产最多 1 条 384；编辑历史/重置不再累积 |
| GC 收敛 | 一次维护循环删到上限以下（单次预算 5 万行），不再每次只删 2000 条 |
| LRU | 读命中攒 touch（同一行 30 秒/200 条批量刷，且 `last_used_at < now-1h` 才写）→ GC 按最近使用淘汰，而不是按写入先后 |
| 维护时机 | 写缓存后触发，6 小时节流（每库各自计时） |
| 孤儿 | 按签名里的 `-v<PIPELINE_VERSION>` 片段收其它版本（`drop_other_pipeline_versions`） |

定稿 / `raw-original` 各有独立缓存键，计入同一库的 2 GiB；超出时按 LRU 淘汰，
需要时重建。15 万张的口径是「每资产 1 条主 384」，定稿变体越多越依赖 LRU。

## 4. 设置与提示

- 设置 → 文件：显示**当前库**的 `thumbs.db` 占用/条数，清理只清当前库；
  无库时按钮禁用。
- 库设置：`photosCount >= 150_000` 时显示「建议开新库」提示（不阻断）。
- 文案走 i18n（中英），不硬编码。

## 5. 测试

- 两个库同名文件身份：各自命中自己的 thumbs.db，不串。
- 库释放 / 换位置：句柄移除；旧库不再被写。
- `put` 只接受 Grid；Screen/Full 被拒；`display::cached_image` 对库内 Screen 不落库。
- 写时清兄弟：base + 多次 `+e` 后只剩最新 1 条；写回 base 后只剩 base。
- GC 一次收敛：5000 行 + 小上限，一次 `gc` 后 `sum(bytes) <= cap`。
- LRU：touch 后 GC 不淘汰最近使用项；一小时内重复命中不重复写。
- 旧全局 `_sources/thumbs.db` 清理：临时目录 + 文件，调用后不存在。
- 150k 容量：`#[ignore]` 手动基准（15 万条 12.75 KB 合成数据）在 Windows 真机跑，
  验证 DB 大小 ≤2 GiB、GC 时间可接受。
- 标签 5000：`ensure_tag` 满额拒绝新名、已有名照常；`ensure_names_capped` 后台跳过；
  `tag_stats` / `tag_prune_unused`。

## 6. 不做 / 后续

- **不**把 384 改成磁盘文件（一图一文件）：见 `memory/FUTURE.md` G27（仅存档）。
  代价是 15 万 inode、GC 扫目录、丢 hits、Windows Defender 扫小文件。
- **不**删/合并已用标签（跨库清 `asset_tags`）；只清 `use_count = 0` 的未使用标签。
- XMP：小图是派生数据，不影响 sidecar；标签上限不改 XMP 映射。
