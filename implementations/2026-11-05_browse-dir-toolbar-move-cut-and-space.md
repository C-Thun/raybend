# 左列目录入口改造 + 照片移动到目录 + 剪切粘贴 + 空格键修复

> 完成时间：2026-11-05 22:27:24
> 需求原文：`todos/2026-11-05-browse-left-dir-entry-and-move.md`（崔总 2026-11-05，附两轮拍板）

## 一、这一批做了什么

| # | 需求 | 结果 |
| --- | --- | --- |
| 1 | 行尾 `⋯` 菜单拆掉，功能搬到常驻工具栏 | 工具栏（**只有两枚图标**：新建 / 删除）+ 行尾 **hover 才出现的标签按钮** |
| 2 | 工具栏不在滚动区内 | 是树的**兄弟**节点，`shrink-0` |
| 3 | 目录点一次选中、再点一次取消 | 再点 = **回库根态**（中列空态水印「当前处于相片库根目录」）；回收站同理 = 退出回收站回库根 |
| 4 | 删除判据实时 enable/disable | 选中变化即跑**浅检查**（两次 `read_dir`），查期间的按钮是暗的 |
| 5 | 拖动照片到别的目录 | pointer 拖动加 `dir` 落点：辅色底高亮、**停 1 秒自动展开**、松手移动 |
| 6 | `Ctrl+X` / `Ctrl+V`（不支持 `Ctrl+C`） | 两条命令 + 进程内剪切态；`Esc` 取消；tile **缩图压到 70%**；裁剪状态写进 flowbar 默认槽 |
| 7 | 空格键切 area 不生效 | 根因已定位并修复（见 §三） |

## 二、关键决策与理由

### 2.1 删除判据改成「浅检查」（崔总口径）

原来要**深度检索**整棵子树（"真的一个文件都没有"），现在只看这一层：

* 有**用户可见的子目录** → 不能删（要删就一级一级删，不搞递归清空目录）；
* 有**照片**（本层 + `_RAW/` 里的，它们在网格里是 tile）→ 不能删；
* `.xmp` 残留、空 `_RAW/`、杂文件 → **看不见，不阻止**，`remove_dir_all` 时一起走；
* 符号链接 → 一律当"有东西"，不跟随也不删。

代码：`crates/raybend/src/repo/dirs.rs`（`check_visible_contents` / `remove_dir_checked`，
删前**自己再查一遍**，不靠调用方记得先查）。DTO 与命令随之改名：
`dir_empty_check` / `dir_remove_empty` → **`dir_delete_check` / `dir_delete`**，
`DirEmptyView` → `DirDeleteCheckView`（旧名字带 "empty" 会误导）。

### 2.2 库根是一种**状态**，不是"能铺图的范围"

`scopePath === null` 本来就不发查询（2026-09-18 的口径），这次把它的**语义写明白**：
它就是"选中了一个目录又被取消"的落点，中列显示「当前处于相片库根目录」。新建目录在
这个状态下落在 **`photos/` 这一级**（`createParentOf`），不是库根目录 —— 库根下建目录
在树里根本看不见。

### 2.3 移动是**文件组搬家**（新模块 `store::relocate`）

后端此前**没有**"目录间移动"能力（只有移入回收站 / 还原）。新增
`crates/raybend/src/store/relocate.rs`：

* 一组文件（位图 + `_RAW/` 原片 + `.xmp` 侧车）整体搬，RAW 断不了；
* 重名整组加 `_01`…（上限 999，已登记 `memory/LIMITS.md`）；
* 任何一个文件动不了 → **整组回滚**；
* 目标校验：必须是 `photos/` 下的真实目录（拒库根、回收站、`_RAW/`、越界）；
* 源目录搬空逐级收掉（`_RAW/` 盖）；
* **复用**回收站的 `files_of` / `move_file` / `rollback` / `prune_ancestors_of`（改成
  `pub(crate)`），没有写第二份；`FileRow` 字段随之开放同 crate 可见；
* **缓存不用清**：小图按文件身份（同卷 rename 身份不变）、大图按 `asset_id` + 源签名（大小/mtime 不变）；
* **不进撤销栈**（与删除一致）。

命令：`browse_move`（`src-tauri/src/browse.rs`）→ `src/api/db.ts::moveAssets` → 契约文件
`MoveResult`（Rust 契约测试 + TS 键表两侧都加）。

### 2.4 flowbar 提示区多了一个**默认槽**

（崔总 2026-11-05）`lib/flow-hint.ts` 新增 `flowHintDefault()` / `setFlowHintDefault(text)`：

* **唯一一份事实文字**，默认空串；`showFlowHint` / `clearFlowHint` 的语义**一个字没改**；
* 优先级：**指向提示 > 默认槽**（鼠标指着某处时盖住状态，手一移开就回来）；
* 默认槽**不受**「提示区开关」管 —— 那个开关管"烦不烦人的说明"，默认槽装的是**状态**
  （关掉提示不该把"你正拿着剪切的东西"也藏起来）。

剪切态用它播报：`已经剪切 {n} 张照片，可用 Ctrl+粘贴到新目录，按 Esc 键取消`。

### 2.5 剪切 / 粘贴

* `features/browse/photo-cut.ts`：模块级单例（与 `photo-drag.ts` 同一种），绑库；
* 命令 `browse.cut`（`Mod+X`）/ `browse.paste`（`Mod+V`），`Mod+C` **不登记**（暂不支持，面板里也搜不到）；
* 粘贴目标 = **当前选中的目录**；库根、回收站、照片原来所在目录都不可用（`canPaste`）；
* `Esc` 取消剪切：走 **capture 阶段**（`preventDefault` 让分发器不跑），所以不会连带清掉选中；
  弹窗开着时让位给弹窗。用这个办法而不是新登记一条同键命令 —— 那会与 `edit.clearSelection` 冲突。

## 三、空格键：根因（确定性 bug）

真实空格键的 `event.key` 是 `" "`（一个空格字符），而 `normalizeKey` 第一步就是 `trim()`
→ 空串 → `null` → **`chordMatches` 永远不成立**。所以 `catalog.ts` 里那条
`defaultKey: "Space"`（编辑区「默认层 ↔ 最近那一层」）**从上线起一次都没触发过**。
单测只测过 `normalizeKey("Space")` 这个方向，反向（真实事件 → 键名）从未被覆盖。

修法：`eventKey()` 先认 `" "` 再走 `normalizeKey`。回归测试两条（`key-chords.test.ts` 的
真实事件形状 + `dispatcher.test.ts` 的分发链路）；附带收获：**快捷键设置现在能捕获空格键**了。

## 四、涉及文件

**前端**
`features/browse/BrowsePanels.tsx`（工具栏 / hover 标签 / toggle / 浅检查 / 1 秒自动展开）、
`features/browse/dirs.ts`（`nextDirScope` / `createParentOf`）、`dirs.test.ts`、
`features/browse/photo-drag.ts`（`dir` 落点）、`features/browse/photo-cut.ts`（新）、
`features/browse/store.ts`（`moveIds` + `BrowseApi.moveAssets`）、`browse/actions.ts`（cut/paste/cancelCut）、
`workspaces/browse/BrowseWorkspace.tsx`（拖放接线 / Esc / toast / cutIds）、
`features/photo-grid/{PhotoGrid,PhotoViewingStage}.tsx`（`cutIds` 透传）、
`components/ui/Tile.tsx`（`cut` → 缩图 `opacity-70`）、
`features/commands/catalog.ts`（两条命令 + deps）、`App.tsx`（接线）、
`lib/{key-chords,flow-hint}.ts` + 两个测试文件、`api/{db,types,dto-contract.json,dto-contract.test}.ts`

**后端**
`crates/raybend/src/store/relocate.rs`（新，7 例单测）、`store/recycle.rs`（5 个 helper 开为 `pub(crate)`）、
`store/mod.rs`、`repo/dirs.rs`（浅检查 + 删除，替换原深度实现与 7 条测试）、
`src-tauri/src/{browse,dirs,contract,lib}.rs`

**文档**
`memory/FUNCTION-BROWSE.md` §4.3、`design/browse.md` §2.2、`memory/LIMITS.md`（重名 999）、
`memory/FUTURE.md`（拖动边缘自动滚动，**未做**）、`src/i18n/index.ts`（key 台账）
（另：`AGENTS.md` 新增 §2.30「执行任何指令前先估磁盘压力」）

## 五、验证

| 门 | 结果 |
| --- | --- |
| `pnpm typecheck` | 通过 |
| `pnpm test` | **1428 passed / 0 failed** |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 三条全绿 |
| `pnpm test:rust`（= `cargo test --workspace`） | **通过**（9 个 test target 全 ok；`contract::every_contract_entry_has_a_test` 先红过一次，补 `MoveResult` 断言后转绿） |

## 六、遗留 / 未验证

* **画稿已同步**（2026-11-05）：`design/browse.pen` 的六处目录树（Tiles / View / Compare /
  Libs Expanded / Recycle / Tiles·拖动删除）都改成了「工具栏 + 行尾无 `⋯`」，并画出了
  「指向行 = `$state-hover` 底 + 行尾 `tag` 图标」与「删除按钮不可用（`opacity: 40`）」两态；
  `States` 里④的空态文案改成「当前处于相片库根目录」。Libs Expanded 的树从 88 → 116 高
  （工具栏不属于滚动区，展开库列表时也要在），库列表相应 716 → 688（内容本来就没顶满，视觉不变）。
* **全部改动未经人类真机确认**：拖动高亮/自动展开的手感、剪切态 70% 的实际观感、
  删除工具栏的启用节奏、`Ctrl+X/V` 与系统输入法的冲突、空格键在编辑区的实际效果 —— 都要崔总在真机上看。
* 拖动到树的**上下边缘自动滚动**没做（`memory/FUTURE.md` 已登记）。
* 回收站里点同一行 = 退出回收站回库根：已在代码里实现，但**回收站 + 反选**这条组合没有自动化测试覆盖（只覆盖了纯函数 `nextDirScope`）。
