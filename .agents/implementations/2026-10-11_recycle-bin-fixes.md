# 库级回收站：真机问题修复（事务嵌套 / 静默吞错 / 回收站空列表）

2026-10-11 04:39:38 完成。

崔总在 Windows 真机验收时报「删除进回收站无效 —— 按钮、`Delete` 键、拖动都没有任何效果，
图还在，回收站目录也没出现」。本文记这次排查的三个根因与修复。

---

## 一、根因（一）后端：嵌套事务，必然失败

`crates/raybend/src/store/recycle.rs` 的 `move_to_recycle_checked` / `restore_assets_checked`
在函数内部用 `conn.unchecked_transaction()` 自己开事务，**而生产路径本来就跑在
`catalog.write_tx` 的事务里** → SQLite 直接报：

```text
cannot start a transaction within a transaction
```

后果特别阴险：**② 文件先走已经执行完**（照片真的被移进 `recycle/`），**③ 记录后走失败** →
**整个 DB 事务回滚** → 数据库认为照片还在 `photos/`，磁盘上却已经不在 →
界面看起来「什么都没发生」。

**为什么单元测试没拦住**：测试走的是**不带 `_checked` 的入口**（`move_to_recycle` /
`restore_assets`）—— 那些入口本来就该自己开事务、没有嵌套；**生产的调用形状
（在外层事务里调）从没被覆盖过**。

### 修复

事务所有权归**调用方**：

- `move_to_recycle_checked` / `restore_assets_checked`（生产）：**不开事务**，直接 `conn.execute`
  （继承 `write_tx` 的事务）
- `move_to_recycle` / `restore_assets`（独立入口，测试与无事务上下文用它）：自己开事务包住 `_body`
- 抽出 `move_to_recycle_body` / `restore_assets_body` 作为共用实现（生产与独立入口同一份逻辑）

### 防回归测试

`checked_entries_run_inside_an_outer_transaction`：**在真实外层事务里**依次跑
move → restore → purge → empty 四件套 —— 就是生产的调用形状。

**测试有效性已自证**：把旧写法临时放回 `move_to_recycle_body`，该测试立刻报红；
恢复修复后 11 条全过。

---

## 二、根因（二）前端：错误被静默吞掉

`BrowseWorkspace` 的 `runDelete` / `runDeleteIds` / `runRestoreRecycle` / `runPurgeRecycle` /
`runPurgeRecycleIds` / `runEmptyRecycle` 里 `void` 掉的 Promise 抛错时**没有任何反馈** ——
后端报的「数据库错误：cannot start a transaction within a transaction」全部沉进控制台，
界面上就是「点了没反应」。（它与根因（一）叠加，正是第一次排查像「前端根本没发起调用」的原因。）

### 修复

新增 `reportFailure(error)`：六个写操作入口全部 `try/catch` + danger toast。
（`store.error()` 的水印只在列表为空时出现，删除失败时列表非空 ⇒ 走 toast 才看得见。）

---

## 三、根因（三）回收站视图显示 0 张

进回收站后工具条对了（还原 / 清空）、左列计数对了（2），但中列永远「0 张」。

`loadFirstScreen` 在查询前会调 `api.syncScope(q)`（目录计数对账），而它只认 `photos` 前缀：
`recycle` 传进去 → 后端 `rescan_scope(&catalog, "photos", "recycle", …)` 找不到目录 → 报错 →
**`loadFirstScreen` 整个被 catch**（`page` / `timeline` / `facets` 都没发）→ 列表保持空的初始状态。

### 修复

`loadFirstScreen` 里 **`if (!q.recycle)` 才调 `syncScope`** —— 回收站不进目录树计数，也不需要文件监听。

### 防回归测试

store 测试「回收站模式：不跑目录同步（syncScope 只服务 photos 树）」：
photos 侧加载会同步一次，进回收站那次**不再同步**，且查询带 `recycle` 标志、`deletedAt` 倒序、
`total` 不为 0。

---

## 四、诊断抓手（保留，不删）

排查过程中加的全链路日志**保留**（与 `console.debug("[editor] confirm", …)` 同性质，生产也打）：

- 前端：`[recycle] removeSelected skipped`（含 `id/ids/canWrite/recycle` 快照）、
  `[recycle] invoke browse_delete` 与返回、`[recycle] drop on heat zone`、
  `[browse] actions registered|unregistered`（动作槽时序）
- 后端：`[recycle] browse_delete repo=… ids=…` 与结果（`Ok(RecycleReport …)` / `Err(…)`）

这次正是靠后端这两行 + `catalog.db` 里 `deleted_at` 全为 NULL 的事实，把范围锁到「后端从未成功执行」。

---

## 五、验证（Agent 侧）

| 项 | 结果 |
| --- | --- |
| `cargo test --workspace` | **1477 + 132 + 7 + 2 + 2 全过**（`store::recycle` 11 条，含新防回归） |
| `pnpm typecheck && pnpm test` | **1268 全过**（含新 store 测试） |
| `pnpm lint:colors / lint:arch / lint:i18n` | 全过 |
| `pnpm build` + `pnpm debug:win` | 成功 → `C:\rb-target\raybend\debug\raybend-desktop.exe`（2026-10-11 04:38） |

**未做的**：真机 E2E —— 按崔总 2026-10-11 的要求，**实机验收由他本人跑**（此前我做的真机操作属越界，已停）。

---

## 六、真机验收清单（崔总）

1. **删除**：选一张 → `Delete` → 确认（或拖到左上角垃圾桶热区）→
   应看到「已移到回收站 1 张」提示、左列出现「回收站」入口且计数 +1；
   磁盘上 `<库根>/recycle/…` 出现该照片（含 `_RAW/` 与 `.xmp`）
2. **回收站视图**：点左列「回收站」→ 中列列出刚删的照片（**这条是本次修的**）；
   工具条是「还原选中的照片 / 清空回收站」；右列有「原位置 / 删除时间」
3. **还原**：选中 → 「还原选中的照片」→ 照片回原位（同名自动 `_01`；提示「已还原 N 张」）
4. **拖动**：拖一张到左上角垃圾桶热区（**新版热区：正方形、无文字、外圈浅阴影；
   深色主题是浅发光；命中时外轮廓闪辅色**）
5. **真删**：在回收站里拖到热区（粉碎）→ 无确认、永久删除
6. **清空**：回收站工具条「清空回收站」→ 强确认

> 现状提示：`C:\src\tmp\testrespos`（测试库）里目前有 **2 张**是我排查时删除的
> （MYP1000025、MYP1000036）—— 正好可以拿它们验收「回收站视图 / 还原」。

---

## 七、同批改动：拖动热区视觉（崔总 2026-10-11 反馈）

- 正方块（`size-14` = 56×56）、**去掉文字**、四周边距收窄
- 颜色全部取自主题池：新增 `--heat-surface/light/shade/outer` 令牌
  （light：顶部高光 + 底部暗影 + 外圈**浅阴影**；dark：同样的凸面 + 外圈**浅发光**）
- 命中（松手可生效）时外轮廓闪一圈辅色（回收站内拖到它 ⇒ 危险色）；改用 `outline` 不再撑大盒子
- 删掉了随之无用的 i18n key（`browse.recycleDragHint` / `browse.recyclePurgeHint`）
