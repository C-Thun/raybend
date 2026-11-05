# 浏览右栏元数据字段：建议输入、就地回填与离线逆地理编码

> 出处：崔总 2026-11-03 的需求 + 当日拍板（5 项）。状态：**已实现（单元测试通过）**，
> 真机 E2E 待崔总；实施记录 `implementations/2026-11-03_browse-metadata-inputs.md`。

## 1. 需求与拍板

| # | 崔总的要求 | 拍板 |
| --- | --- | --- |
| 1 | 导入或导出时自动设置作者、描述等元信息 | 登记 `memory/FUTURE.md` **H7**（扩写为「库级元数据模板」），本期不实现 |
| 2 | 右栏填写点要有更优的填入方式 | 建议输入（最近 20 次 ∪ 库内已有值），前缀筛选 + ↑↓ + Enter |
| 3 | 作者/描述这类「没有数据来源、靠人定」的项，保留最近 20 次输入 | 做；上限登记 `memory/LIMITS.md` |
| 4 | 手机相片的真实经纬信息如何表现 | GPS 只读 + 复制 + 地图；「按 GPS 填入地点」用离线城市表 |
| 5 | 批量写入语义 | **A**：右栏只改锚点，多选时显式「应用到所选 N 张」 |
| 6 | 描述编辑形态 | 行内简版 + 展开弹窗（多行、保换行） |
| 7 | 候选来源 | 包含「库内已有值」 |
| 8 | 逆地理编码 | 不接受后置，**本期做掉** |
| 9 | 读别家 XMP | 暂不放开；只读**自家** sidecar，登记 `memory/FUTURE.md` **H11** |

## 2. 问题（崔总实测的 1 秒延迟）

`EditableField` 提交 → `store.setText` → `store.mark`。`mark()` 只回填标记四项，
**文字字段没有回填**，于是回车后右栏显示旧值。界面之所以最终会更新，是一条偶然旁路：

```text
browse_mark → sidecar 写 .xmp → fs_atomic 先写 <名字>.tmp-<pid>-<n>
→ 文件监听只排除扩展名 "tmp"，把 .tmp-1234-0 当成照片变更
→ 200ms–1s 去抖 → catalog://dirty → 整目录重扫 + 首屏重读（refresh 里还会再同步一次目录）
```

副作用：**XMP 写出开关关闭时，文字字段永远不更新**；评级/标签/编辑确认也各自触发一次整目录重扫。

## 3. 方案（已实现）

### 3.1 写库后**就地回填**（P0）

- `store.setText` 只写**锚点那张**；`store.setTextForSelected` 写全部选中（批量按钮用）。
- 成功后按 `skippedLocked` 过滤、按后端同口径（`trim`、空串→NULL）回填 `entries()`：
  右栏立即显示新值，不等任何后台链路。实现 `features/browse/text-edit.ts`（纯函数 + 单测）。
- 失败不吞：调用点 catch 后走 Toast（`AGENTS.md` §2.29）。

### 3.2 文件监听忽略原子写临时文件（P0）

`media/watch.rs::is_internal_file` 把 `tmp-<pid>-<n>` 认作内部文件（另有 `tmp/db/db-wal/db-shm/xmp`）。
sidecar 写入不再触发目录重扫；补了「真扩展名 `tmpfoo` 不能被误杀」的测试。

### 3.3 建议输入（P1）

- 新基础组件 `components/ui/SuggestInput.tsx`（focus 出列表、前缀筛选、↑↓、Enter 采用、
  Esc 先关列表；Portal + fixed 定位；8 行可见后滚动）。
- 纯逻辑在 `lib/suggest.ts`：两源合并（最近输入在前、库内值按次数）、前缀优先/子串兜底、上下键不回绕。
- 最近输入：`lib/text-history.ts`（设备级 localStorage，版本化 key `raybend.textHistory.v1`，
  每字段 20 条、MRU 去重；写入成功后才记）。
- 库内已有值：`browse_text_values` 命令 → `query::text_values`（按字段 + 上级地理维度过滤，
  上限 `TEXT_SUGGEST_LIMIT = 20`；**描述不统计**，只来自最近输入）。
- 地理字段逐级联动：国家 → 省（按国家）→ 市（按省）→ 具体地点（按市）。

### 3.4 描述：行内 + 弹窗（P1）

无换行的值走行内编辑；含换行的值点击直接开弹窗（单行 `<input>` 会吃掉换行）。
弹窗是 `Dialog` + `textarea`，取消左/确认右，回车=换行。

### 3.5 批量与字段归位（P1）

- 多选时字段行尾出现「应用到所选 N 张」（IconChecks + 提示），写入全部选中并给成功 Toast。
- 尺寸行从「拍摄信息」移到「文件基础信息」（随显示变体，与 flowinfo 同源）。
- GPS 只留「地理信息」一处（删除 EXIF 段重复行），十进制 6 位 + `EasyCopy` + OpenStreetMap；
  **没有坐标时整行不渲染**（2026-11-04 崔总：GPS 只从文件 EXIF 来、不提供手填，
  所以不能挂「未填写（点这里编辑）」那句 —— 点了什么都不会发生；空态也不留占位）。
  那条「未填写（点这里编辑）」只属于**可编辑**字段（作者/描述/国家/省/市/具体地点）。
- 输入框带 `maxLength = TEXT_FIELD_MAX_CHARS (2000)`；Enter/Esc 判 `isComposing`（中文输入法）。

### 3.6 离线逆地理编码（P2，本期做掉）

- 数据：`scripts/build-geo-data.mjs` 从 GeoNames dump 生成
  `crates/raybend/assets/geo/{cities,admin1,countries}.tsv`（约 1.7 MB，34 197 城，
  其中 12 357 个有中文名；国家名走 CLDR/`Intl.DisplayNames`）。**随仓库发布**，不运行时下载
  （GeoNames dump 每天重生成、无稳定版本号，无法固定哈希）。
- 核心：`crates/raybend/src/geo.rs`（`include_str!` + `OnceLock` 解析一次，haversine 最近城，
  同距离取人口多的；中文请求缺中文名时退回英文）。
- 命令：`geo_reverse(lat, lon, locale) -> GeoPlace | null`（`src-tauri/src/geo.rs`）。
- 写库：`browse_mark` 的 `setGeo` → `marking::fill_geo`（**只填空字段**，三项合成**一步撤销**）。
- 界面：地理段「按 GPS 填入地点」按钮（有坐标且三项有空缺才出现），成功 Toast，失败走统一通道。
- 许可：GeoNames 数据 CC BY 4.0（`legal/CC-BY-4.0.txt`、`THIRD-PARTY-NOTICES.md`、
  `crates/raybend/assets/geo/README.md` 署名）。

## 4. 代码地图

| 面 | 文件 |
| --- | --- |
| 右栏界面 | `src/features/browse/BrowsePanels.tsx`（`EditableField` / `DescriptionField` / `DescriptionEditor` / `GpsField` / `AssetInfo`） |
| 建议输入 | `src/components/ui/SuggestInput.tsx`、`src/lib/suggest.ts`、`src/lib/text-history.ts` |
| 就地回填 | `src/features/browse/text-edit.ts`、`src/features/browse/store.ts` |
| 库内已有值 | `crates/raybend/src/store/query.rs::text_values`、`src-tauri/src/browse.rs::browse_text_values` |
| GPS 填入 | `crates/raybend/src/store/marking.rs::fill_geo`、`crates/raybend/src/geo.rs`、`src-tauri/src/geo.rs` |
| 监听过滤 | `crates/raybend/src/media/watch.rs::is_internal_file` |
| 外链打开 | `src-tauri/src/open_url.rs`、`src/api/external.ts` |

## 5. 验收（单元测试已覆盖；E2E 待崔总）

1. 关掉 XMP 写出开关后改作者/描述，右栏**立即**更新。
2. 改一次文字**不产生**目录重扫（日志无 `catalog://dirty`）。
3. 中文输入法回车确认候选不会提前提交。
4. focus 出建议列表、前缀筛选、↑↓ + Enter、Esc 只关列表（再按取消）。
5. 描述带换行的值编辑后不丢换行。
6. 多选时字段行尾出现「应用到所选 N 张」，落库后 Toast 提示。
7. GPS「按 GPS 填入地点」只填空字段、一次撤销全部退回。
8. `cargo test --workspace`、`pnpm test`、`pnpm typecheck`、`pnpm lint:*`、`pnpm build` 全绿。

## 6. 未做 / 后续

- `design/browse.pen` 的三态帧**尚未补**（本会话 Pencil 应用未连接）：行内建议列表展开态、
  描述弹窗、批量按钮；文字规格已写进 `design/browse.md` §2.4。
- 导入时从原图 EXIF Artist / ImageDescription 取作者/描述（H7 相关，需单独评估）。
- 城市表会随 GeoNames 变旧；重建命令见 `crates/raybend/assets/geo/README.md`。
