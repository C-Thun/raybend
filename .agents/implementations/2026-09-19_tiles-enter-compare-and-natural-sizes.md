# tiles 多选进对比 + 老库宽高补齐（对比「没读到尺寸」的修复）

完成时间：2026-09-19 17:59:31 CST

## 人类报的两条

> 1. 「在 browse 下对比其实是有的，但首先肯定是不支持 tiles 下多选按回车进对比，
>    只能在 film 下 Ctrl+多选，这个肯定是不行的」
> 2. 「现在以 film 下 Ctrl+多选虽然能进入对比模式，但会显示出来『还没读到这张的尺寸』，也是有 bug 的」

两条都修了，并且第 2 条**挖到了根因**。

## 一、tiles 下多选 + 回车 → 对比

**现状**：回车只在「恰好选中 1 张」时才进看图（`selected.length !== 1` 直接 return），
多选按回车什么都不发生 —— 两条网格（导入 / 浏览）都是这么写的。

**改法**：把「1 张」这层限制去掉，取**锚点**那张作为进看图的位置；
选中 ≥ 2 张时进看图**就是对比态**（对比是选择状态的派生值，见工作区的 `comparing()`），
所以不需要第二条代码路径。

* `src/features/browse/BrowseGrid.tsx` 的 `onGridKeyDown`
* `src/features/photo-grid/PhotoGrid.tsx` 的键盘处理（导入侧同一条口径）

## 二、「还没读到这张的尺寸」—— 根因是老库的 `assets.width/height` 是 NULL

查真实库（人类那台机器的 `testrespos/catalog.db`，拷到 `/tmp` 读，**不动原文件**）：

```text
assets = 10        有宽高 = 2        宽高缺失 = 8        方向缺失 = 8
id 1..8  imported_at = 09-16 07:26   ← 全是 2026-09-18「导入写 EXIF」那次修复之前入库的
id 9..10 imported_at = 09-18 17:43   ← 修复之后导入的，宽高都在
```

⇒ 与 `store/backfill.rs` 文件头写的完全一致：**2026-09-18 之前的导入不写 EXIF**，
而这批老资产至今没被回填（回填模块写了，但**没有任何地方调用它**）。
后果有两层：

1. tile 比例退回占位 → 这就是人类一直在报的「browse tiles 显示不对」；
2. 对比画幅算不出基准比例 → 「还没读到这张的尺寸」。

### 这一轮做的（兜底层）

* **浏览 store 新增 `naturalOf(id)` / `ensureNatural(entries)`**（`features/browse/store.ts`）：
  与导入侧同一套纪律 —— 只补缺的、合并进缓存（不清空）、失败静默；按目录分组、一个目录一次 IPC。
* **注入点**：`BrowseDeps.metaEnsure`（`App.tsx` 传 `db.dirMetaEnsure`）。
* **什么时候补**：
  * 滚到哪补哪（`BrowseGrid` 的 `onVisibleRange` 里，`ensureRange` 之后顺手补这一段）；
  * 进对比时补**对比那几张**（浏览工作区的 effect，通常 ≤4 张）。
* **谁消费**：`BrowseGrid` 的 tile 比例与看图列表、浏览工作区的对比画幅 —— 全部改读
  `store.naturalOf(id)`，于是补读到的值立刻生效（不再用进看图那一刻的快照）。
* 收了一处重复：`absPath`（根 + 相对路径拼接）原先在浏览网格与工作区各写一份，
  现在收进 `lib/paths.ts` 的 `joinPath`（顺带把 Windows 根的混合分隔符规范掉），带单测。

⚠️ **兜底不等于修好**：真正的修复是**跑回填**（把老库那 8 个资产的 EXIF 重新读一遍）。
回填模块 `store/backfill.rs` 至今**没有任何调用点** —— 这条登记到「重建数据」那件事里做
（人类 08:00 那条要求的「齿轮弹窗里带重建数据」正好是它的入口）。

## 三、验证

```text
pnpm typecheck → 0
pnpm test → 694 passed（新增 lib/paths 6 条、browse store 的宽高兜底 5 条）
pnpm lint:arch / lint:colors / lint:i18n → 全绿
pnpm build → 通过
pnpm check:browse → 通过，新增两条断言：
  · tiles 里 Ctrl 点两张 + 回车 → 直接进对比（2 幅画幅）
  · fixture 里最后两张**故意不给宽高**（老库形态），对比画幅仍要出图、
    且不许出现「还没读到这张的尺寸」
pnpm smoke:ui → problems: []
```

**未经人类验证**：真机上老库那批照片进对比的观感、以及补读带来的等待感
（老库第一次滚到某段时会有一次读盘）。

## 四、过程记录

* `lib/paths.ts` 第一版的注释写的是「分隔符跟着根走」，但实现只换了**拼接处**那一个分隔符 ——
  测试当场把三处 Windows 用例打红，改成「相对路径里的分隔符一律规范成与根同款」。
* 浏览 store 的宽高索引第一版用了 `createMemo`：在 Node 里 `solid-js` 走 SSR 构建（**没有响应式**），
  memo 只算一次 ⇒ 索引永远是空表，测试表现为「DB 里明明有宽高，`naturalOf` 却总返回 null」。
  改成**按数组身份**缓存的惰性索引（`setEntries` 每次换新数组，身份一变就重建）。
