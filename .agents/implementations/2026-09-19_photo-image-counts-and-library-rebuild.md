# 相片/图片数量体系 + 库设置弹窗（两个总数 + 重建数据）

完成时间：2026-09-19 18:47:05 CST

## 范围（人类 2026-09-19 08:00 那条「老生常谈」里的数据侧）

那条要求里有三块：**数量体系**、**库卡片统一**、**齿轮弹窗 + 重建数据**。
本轮把前两块的数据基础与第三块整体做完；卡片的**代码级合并**留下一段（见「未完」）。

## 一、数量体系（人类原话 → 实现）

> 「系统中有两个概念`相片数量`和`图片数量`，其中相片数量指不包括 `_RAW` 目录下文件的数量，
> 而图片数量指包括 `_RAW` 下的数量……库有两个字段 `repository.photos_count` 和
> `repository.images_count`……目录下的统计只计本身目录内和本目录下的 `_RAW`，不包括其他子目录」

* **schema（app.db v4）**：`app_0004_directory_counts.sql`
  * `repositories.photos_count` / `images_count`（库级**汇总缓存**）
  * `directories(repository_id, rel_path, rel_path_folded, photos_count, images_count, updated_at)`,
    主键 `(repository_id, rel_path_folded)`
* **store 层**（`store/repository.rs`）：`Counts` + `count_dir_on_disk`（本目录 + 本目录的 `_RAW`；
  `catalog.db` / `.xmp` / 隐藏文件不算照片）、`count_library_on_disk`（递归逐目录统计并落库）、
  `set_directory_counts`（写一行 + **同事务内**重算库级汇总）、`totals`、`clear_directories`。
  `build_views` 不再逐个打开库的 `catalog.db` 数资产 —— 两个数字直接读 app.db。
* **三条写入路径**（人类指定的三条）：
  1. **导入结束**：`refresh_directory_counts` 重扫 `photos/` 逐目录统计（模版可变、子目录透传不定，
     从结果反推比从参数正推稳）；
  2. **进目录时**：新命令 `repository_sync_dir(repositoryId, scopePath)` 读盘数一次、
     与库里那行对比、不一样就写回并把差值滚到库级汇总（本地 `readdir` 微秒级 ——
     `AGENTS.md` §2 #13「实时性优先」）；
  3. **重建数据**：清空后全量重算（见下）。
* **老库兜底**：库列表里遇到「在线但还没数过」的库，顺手扫一次盘并把结果落库 ——
  否则老库的卡片永远是「—」。

## 二、重建数据（齿轮弹窗里的那个按钮）

新命令 `repository_rebuild(repositoryId)`，四步（顺序不能反）：

1. **扫盘 + 对齐 catalog**：`store/rebuild.rs` 的 `rescan_library`
   （`media::scan` → `media::diff` → `assets::apply_diff`）—— 登记新文件、标记缺失、修正路径；
2. **重读元数据**：复用 `store/backfill.rs`（老库那批 `taken_at`/宽高为 NULL 的资产）——
   这正是人类库里 8/10 个资产没有宽高的根因；
3. **重算目录计数**：清空 `directories` 后按磁盘重数；
4. **汇总**进 `repositories`（第 3 步顺带完成）。

`rescan_library` 里有个**必须记住的坑**：扫描给的相对路径是「相对 `photos/`」，
而库里存的是「相对库根」——**少补一层前缀，重建一次库里就会多出一整套重复照片**。

## 三、齿轮弹窗（库设置）

* 新增两个总数：**相片数量**（不含 `_RAW`）/ **图片数量**（含 `_RAW`）+ 一句口径说明；
* **重建数据**按钮 → **二级确认**（`Dialog`/`ConfirmDialog` 新增 `scrim?: boolean`：
  二级传 `false` = **透明遮罩**，人类明确要求「不要半透叠半透」）→ 跑完把结果写成一句人话
  （扫到多少 / 新登记 / 标缺失 / 补元数据 / 现在的两个数）；
* 弹窗自己读一次 `repository_counts`（浏览侧只拿得到 id），外面的视图有值就优先用外面的。

## 四、库卡片（外观与语义统一）

浏览左列的紧缩卡片按导入侧的样子重做：**图标块 + 库名 + 路径 + 相片总数**，
行尾一格：**在线 = 齿轮**（开同一个库设置）、**离线 = 离线图标**（重新查找）。
点卡片主体仍然是「选库 / 移顶」的语义。

## 五、验证

```text
cargo test -p raybend → 804 passed; -p raybend-desktop → 44 passed（含新的契约断言）
cargo clippy -p raybend -p raybend-desktop --all-targets → 无告警
pnpm typecheck → 0 · pnpm test → 694 passed · 三个 lint → 全绿 · pnpm build → 通过
pnpm check:browse / pnpm smoke:ui → problems: []
```

**未经人类验证**：卡片与弹窗的观感、重建在真实大库上的耗时与进度体感、
以及「进目录同步计数」在慢盘上的手感 —— 都是目视/体感范畴。

## 六、未完（登记，避免以为做完了）

1. **卡片组件本身仍是两份实现**（`features/repositories/RepositoryList.tsx` 与
   `features/browse/BrowsePanels.tsx` 里各一段）：这轮统一了**外观与语义**，
   但没有抽成一份 —— 分层上它要落在 `components/ui/`，而那里不允许 import api 类型，
   得先把卡片改成「纯 props（名字/路径/两个数/online/回调）」再搬。**这是下一步的第一件事**。
2. **重建没有进度反馈**：现在只有「开始 → 转圈 → 结果一句话」。大库上应当走事件流
   （与导入进度同一套 `emit` 机制）。
3. `_RAW` 三形态标签（`+RAW` vs `RAW`）与 browse tiles 显示问题的**数据根因回填**
   （已提供入口：重建数据里第 2 步）—— 前者未开工，后者要在真机上跑一次重建验证。

## 七、过程记录

* **悬空调用**：`store/backfill.rs` 从写出来那天起**没有任何调用点**（老库回填一直没跑过）——
  本轮把它接进了「重建数据」。
* **同名影子 bug**：浏览工作区里我把本地函数也叫 `remountRepository`，与 import 进来的同名
  ⇒ 函数自己调自己，类型退化成 `void`。编译器的报错只说「类型不匹配」，花了点时间才看出是影子。
  已改名 `remountLibrary` 并在注释里写了这条。
* **死锁风险**：`DbState::with` 持锁期间不能再去叫 `views()`（它内部还要 `db.read`/`db.write`）——
  建库与重挂载两条路径都改成「先落库、出锁、再取视图」。
* **陈旧快照**：`repo.*` 这批新键被检查器连续报「不存在」。做了**负控 + 正控**：
  故意写一个不存在的键 → `tsc` 报错（证明检查路径是有效的）；恢复后 → `tsc` 退出码 0；
  再补一条**运行期**断言（两个语言包都读到了真实文案）。结论：陈旧快照，未改代码。
