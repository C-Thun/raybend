# 浏览右栏元数据字段：就地回填、建议输入与离线逆地理编码

> 完成时间：**2026-11-04 02:31:40 +08:00**（需求与开工在 2026-11-03，收尾跨过零点）。
> 规格：`specs/browse-info-fields.md`；拍板：崔总 2026-11-03（5 项，见规格 §1）。

## 1. 改动范围

### P0 修根因（1 秒延迟 / 「以为功能坏了」）

- **写后就地回填**：`store.setText` 只写锚点，成功后按 `skippedLocked` 过滤并把值
  （`trim`、空→NULL，与 `marking::set_text` 同口径）直接贴回 `entries()`。
  右栏立刻显示新值 —— 不再依赖「sidecar 写盘 → 文件监听 → 整目录重扫」这条偶然旁路。
  新增 `setTextForSelected`（批量）与 `applyGeo`（三项一次撤销）。
- **文件监听忽略原子写临时文件**：`media/watch.rs::is_internal_file` 把
  `tmp-<pid>-<n>`（`fs_atomic::write` 的名字）与 `tmp/db/db-wal/db-shm/xmp` 一并排除。
  在此之前，**改一次作者 / 标一颗星 / 加一个标签都会触发一次整目录重扫**
  （`catalog://dirty` → `repository_sync_dir` → `refresh` 里再同步一次）。
- **中文输入法守卫**：Enter / Esc 判 `isComposing`。
- **失败不再静默**：写库异常走 `Toast`（组装层把 `toast` 传进右栏）；`store` 不吞错误。
- **描述保换行**：单行值走行内编辑，含换行的值点击直接开多行弹窗（单行 `<input>` 会吃掉换行）。

### P1 填入方式

- 新组件 `SuggestInput`：focus 出列表、前缀筛选、`↑/↓`、`Enter` 采用、`Esc` 先关列表；
  Portal + fixed 定位（右栏是滚动容器）、8 行可见后滚动。
- 候选两源：**最近 20 次输入**（设备级 localStorage，版本化 key `raybend.textHistory.v1`，
  MRU 去重；写库成功才记）∪ **库内已有值**（`browse_text_values` → `query::text_values`，
  按字段 + 上级地理维度过滤；描述不统计）。
- 地理逐级联动：国家 → 省（按国家）→ 市（按省）→ 具体地点（按市）。
- 批量显式化（崔总选 A）：多选时字段行尾出现「应用到所选 N 张」。
- 字段归位：尺寸移到「文件基础信息」；GPS 只留「地理信息」一处，十进制 6 位 +
  `EasyCopy` + OpenStreetMap（`open_external_url`，只放行 https）。
- 输入框 `maxLength = 2000`（`TEXT_FIELD_MAX_CHARS`）。

### P2 离线逆地理编码（本期做掉）

- 数据：`scripts/build-geo-data.mjs` 从 GeoNames dump 生成
  `crates/raybend/assets/geo/{cities,admin1,countries}.tsv`（1.7 MB，34 197 城 / 12 357 个有中文名；
  国家名走 CLDR）。**随仓库发布**，不在运行时下载 —— GeoNames dump 每天重生成、无稳定版本号，
  下载端无法固定哈希校验；2 MB 静态表一次付清、离线可用。
- 核心 `crates/raybend/src/geo.rs`（`include_str!` + `OnceLock`，haversine 最近城，
  同距离取人口多的；中文缺名时退回英文）+ 命令 `geo_reverse`。
- 写库 `marking::fill_geo`（**只填空字段**；三项一个 `ChangeSet` ⇒ **一步撤销**），
  动作 `browse_mark` 的 `setGeo`。
- 界面：「按 GPS 填入地点」按钮（有坐标且三项有空缺才出现），成功 Toast、失败走统一通道。
- 许可：CC BY 4.0（`legal/CC-BY-4.0.txt`、`THIRD-PARTY-NOTICES.md`、`assets/geo/README.md`）。

### 文档 / 登记

`memory/LIMITS.md`（2000 字 / 20 条）、`memory/COMPONENTS.md` + `AGENTS.md` §12（`SuggestInput`）、
`AGENTS.md` §13（右栏元数据/地理影响索引）、`memory/FUNCTION-BROWSE.md` §6、
`memory/FUTURE.md`（H7 扩写 + 新增 H11「只读自家 XMP」）、`design/browse.md` §2.4、
`legal/README.md`、`specs/browse-info-fields.md`。

## 2. 涉及文件

#### 前端
- `src/features/browse/BrowsePanels.tsx`（右栏：可编辑字段 / 描述弹窗 / GPS / 尺寸归位 / Toast）
- `src/features/browse/store.ts`（`setText` / `setTextForSelected` / `applyGeo` / `textValues` 缓存）
- `src/features/browse/text-edit.ts`（+ 测试）：回填口径、`fillGeoFields`
- `src/components/ui/SuggestInput.tsx`、`src/lib/suggest.ts`（+ 测试）、`src/lib/text-history.ts`（+ 测试）
- `src/api/{browse,types,external}.ts`、`src/api/dto-contract.json`、`src/api/dto-contract.test.ts`
- `src/workspaces/browse/BrowseWorkspace.tsx`（传 `toast`）、`src/App.tsx`（API 装配）
- `src/i18n/{zh-CN,en-US,index}.ts`

#### Rust
- `crates/raybend/src/geo.rs`（+ 7 个测试）、`crates/raybend/src/lib.rs`
- `crates/raybend/assets/geo/*`（生成数据 + README）
- `crates/raybend/src/media/watch.rs`（+ 测试）
- `crates/raybend/src/store/{marking,query}.rs`（`fill_geo` / `text_values` + 测试）
- `src-tauri/src/{geo,open_url}.rs`、`src-tauri/src/{browse,lib,contract}.rs`

#### 脚本 / 文档
- `scripts/build-geo-data.mjs`
- `specs/browse-info-fields.md`、`implementations/2026-11-03_browse-metadata-inputs.md`
- 见 §1 的文档/登记清单

## 3. 关键决策与理由

1. **就地回填而不是「等刷新」**：原路径把界面正确性押在「sidecar 写盘被文件监听发现」上，
   关掉 XMP 写出就完全不更新；回填是唯一与「库已是真相」自洽的做法，也顺手删掉了
   「改文字 = 两次目录重扫」的暗耗。
2. **监听器按文件名形态排除临时文件**，而不是改 `fs_atomic` 的命名：临时名是原子写的内部细节，
   改它会影响所有写入方；在监听侧排除影响面最小，且补了「不能误杀真扩展名」的测试。
3. **锚点写 + 显式批量**（崔总选 A）：右栏看着谁改谁；一次误操作不至于改一片。
4. **建议输入手搓而不用 Ark Combobox**：这里的值是**自由文本**、候选只作加速，
   Ark 把「选中集合」当第一公民，会与「失焦即提交 / 回车即提交」互相拉扯；
   键盘模型与 `TagDialog` 的输入即搜一致，筛选/合并逻辑抽到 `lib/suggest.ts` 可单测。
5. **数据集随仓库发布**：见 §1 P2；代价是仓库 +1.7 MB 与「地名会变旧」，换来确定性与离线可用。
6. **「按 GPS 填入地点」只填空字段、三项一步撤销**：手写的值最贵，不能被推断覆盖；
   一次用户动作应该一次撤销说得清。
7. **外链只放行 https**（`open_url.rs`）：URL 来自界面字符串，不加白名单等于给界面
   一条越过应用的执行通道。

## 4. 验证

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **全绿**：1643 passed（raybend lib）/ 199 passed（raybend-desktop）+ 其它目标，0 failed |
| `pnpm typecheck` | 通过 |
| `pnpm test` | **1411 passed / 0 failed**（新增 21 + 5 条） |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 三条全通过 |
| `pnpm build` | 构建成功（仅既有 chunk >500 kB 提示） |

新增测试要点：监听器临时文件排除与误杀边界、`text_values` 计数/级联/空值、`fill_geo`
「只填空 + 一步撤销 + 无空字段为空补丁」、`set_text` 幂等与清空、geo 解析/最近城/中文回退/
内嵌数据冒烟（北京/杭州/纽约）、`SuggestInput` 的合并/前缀/上下键纯逻辑、最近输入存储消毒与上限、
回填口径（trim/空→NULL/只改命中 id/锁跳过不回填）。

## 5. 遗留问题（如实）

1. **`design/browse.pen` 未补帧**：本会话 Pencil 应用未连接（`pencil_get_app_state` 连不上
   VS Code 扩展），只更新了配对文字规格 `design/browse.md` §2.4。需要补三帧：建议列表展开态、
   描述弹窗、批量按钮。
2. **未做真机 E2E**：按 `AGENTS.md` §2.8，GUI 交互 / 输入法 / 真机延迟由崔总验证。
   建议重点看：关掉 XMP 写出后改作者是否立刻显示；改一次作者后日志里是否还有
   `catalog://dirty`；中文输入法回车。
3. **导入仍不读原图 EXIF Artist / ImageDescription**（写作 H7 的前置条件，见 FUTURE）。
4. **城市表会旧**：重建命令 `node scripts/build-geo-data.mjs`（见 `assets/geo/README.md`）。
5. **工作树里还有其它会话的未提交改动**（编辑器/LUT/设计稿等）：本次只改上述文件，
   没有提交、也没有动别人的改动。
