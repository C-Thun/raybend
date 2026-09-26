# 导出画廊「整段开关」核实 + 死参数清理（含对一次误报的更正）

完成时间：2026-09-27 03:23:22 CST

## 范围

人类 2026-09-27 批准：「那个问题你改掉吧，就是 export mid top 的 tiles 按日期状态下的选天/段的行为」。

动手前先核实，结论与我的预期**相反**，所以本次交付分两部分：

1. **更正一次误报**（我上一轮说导出侧「只加不减」——**不成立**）；
2. **删掉那个导致误报的死参数** + **补一条适配器级回归**。

## 核实结果（关键证据）

`ExportStore.group`（`src/workspaces/export/store.ts:343`）：

```ts
setSelection((old) =>
  additive
    ? toggleGroupSelection(old, orderedKeys(assets))   // ← additive = true 就是「整段开关」
    : selectAll(orderedKeys(assets)),                   // ← false 才是「只留这一组」
);
```

- **`additive = true` → 共享的 `toggleGroupSelection`**（全选中 → 全取消，否则 → 全开）——
  即 `BROWSE.md` §5.2.2 要求的那条开关，**行为本来就是对的**。
- `additive = false` → `selectAll`（替换）—— 那是 `selectAllActive`（Mod+A）用的，
  不是日组/时间片药丸用的。
- 导出线**已经接上了共享实现**（`store.ts:5` 导入 `toggleGroupSelection`），
  并已有回归用例「日期/时间片使用共享整组开关」（`store.test.ts:273`，
  断言 `group([1,2])` → 6、再 `group([1])` → 3、再点 → 6、最后 `group([1,2])` → 0）。

**我当时错在哪**：只看到调用点写 `additive = true` / `additive` 透传，就下了
「即『只加不减』」的结论 —— **没有读 `group` 的函数体**。
那条被写进 `PLAN.md` §M4-W2 当成收口项，等于把一次未验证的推断当成了事实。
**教训：报告「行为不一致」前必须读实现体，不能从调用点猜。**

## 本次实际改动

| 文件 | 改动 |
| --- | --- |
| `src/workspaces/export/source.ts` | 删掉 `selectGroupRange` 上的**死参数** `additive = true`：`TilesSource` 接口早已只有两个参数（`additive` 在 2026-09-26 的选择语义收敛里被移除），调用点也不再传第三个 —— 留着它既无作用、又是把我误导的诱因。改成 `store.group(ids)`（不传第二参 = 整段开关），并就地写清「药丸 = 开关 / `additive = false` 是 `selectAllActive` 的用法」 |
| `src/workspaces/export/source.test.ts` | **新增适配器级回归**：「日组/时间片药丸 = 整段开关（再点一次取消），不是「只加不减」」—— 断言经适配器点一次 → 9（3 张 × 3 定稿）、同段再点 → **0**、点前两格 → 6。此前只有 store 级用例，**适配器那一段接线没有钉子** |
| `PLAN.md` | §M4-W2 的收口项 ② 划掉并标注 **✅ 已落地**，附**更正**说明与教训 |

**没有改**：`ExportStore.group`（它的两个分支都对）、日组/时间片药丸的语义、任何 UI 视觉。

## 验证

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | ✅ 无错误 |
| `node --test "src/lib/export-model.test.ts" "src/workspaces/export/*.test.ts"` | ✅ **50 通过 / 0 失败** |
| `pnpm test` | ✅ **1071 通过 / 0 失败**（原 1069；本次共 +2 条） |
| `pnpm lint:arch` / `lint:i18n` / `lint:colors` | ✅ 三条全过 |

**未做**：真机 GUI 目视（药丸点两次的视觉反馈）—— 按 `AGENTS.md` §2.8 归人类。

## `JSON.parse(key)[1]` 收口（人类 2026-09-27 批准后已做）

自动化检查把 `src/workspaces/export/source.ts:93` 标成 🔴（`JSON.parse` 未包 try/catch）。
核实结论：**不是可达 bug**（`anchor` 只由 `variantKey()` 产生；`variantKey` = `JSON.stringify([repository, assetId, variant])`，
本处取 `[1]` 正是 `assetId`，往返自洽）—— **但它是同一个元组形状被三处各自位置耦合地解析**：

| 位置 | 原写法 |
| --- | --- |
| `export/source.ts` 的 `selection()` memo | `String((JSON.parse(anchor) as [string, number, string])[1])` |
| `export/store.ts` 的 `setSelection`（ids 过滤） | `(JSON.parse(key) as [string, number, string])[1]` |
| `export/store.ts` 的 `setSelection`（anchor 保留判断） | `(JSON.parse(old.anchor) as [string, number, string])[1]` |

两个真实风险（不是“防守代码洁癖”）：

1. **形状一变不会抛错，而是静默取到 `undefined`** —— 界面上只表现为“锚点丢了、不高亮”，很难察觉；
2. 旧写法在 `createMemo` 与 `setSelection` **内部**抛异常 ⇒ 打坏渲染 / 卡住状态更新；
   而 `ExportWorkspace.tsx` 那处还有更弱的 `key.startsWith("[")` 前缀猜测。

**改法（`AGENTS.md` §2.12 正解：收口形状，不是就地贴 try/catch）**：

- `src/lib/export-model.ts` 新增与 `variantKey` **成对的** `variantAssetId(key): number | null`
  —— 形状只由这一对函数拥有；非 JSON / 非数组 / 第 1 位不是安全整数 ⇒ `null`，**永不抛**；
- 上述**三处**全部改调它；`store.ts` 里那两处重复解析顺带收敛成一个 `dropped(key)` 谓词
  （语义不变：`removed.has(asset) && !allowed.has(key)` 才剔；**解析不出来 ⇒ 保留**，
  不因解析失败丢用户已选的东西 —— 这也正好补上了旧写法“抛出去”时未定义的那条路径）；
- `src/lib/export-model.test.ts` 新增边界用例：空串 / `"null"` / 非 JSON / `[]` / 少一位 /
  字符串数字 / 对象 / 非整数 / `1e999` 溢出 / 超安全整数范围 / 多尾巴 / 中文仓名 / `assetId = 0`。

**验证**：重跑 LSP 扫描，那条 🔴 已消失（**消除成因，不是压制**）。
但**不能说 pi-lens 就“干净”了**：它另有 **16 条既有警告**（非空断言 `!` ×7、
`filter().map()` ×8、嵌套三元 ×2），**没有一条在本次改动的行上**，也未本次授权范围 ——
属既存风格建议，未动。

### 尚未处理（同类、但**是另一种键**，已报告待批）

`src/workspaces/export/ExportWorkspace.tsx:100` 解析的是**队列图片键**
（`[repositoryId, reference, profileHash]`，与 `variantKey` 形状不同），守卫是
`if (!key.startsWith("[")) return getThumbBytes(key, "grid")` —— **前缀猜测，不是有效性校验**
（`"["` 这种输入会让 `JSON.parse` 直接抛）。它同样缺少与生产者成对的访问器。
本次**未动**：不在本次批准范围，且 `ExportWorkspace.tsx` 是导出线最活跃的文件。

## 未提交

工作区里另有 M4/M5 的在改内容，不代人类决定提交边界。
