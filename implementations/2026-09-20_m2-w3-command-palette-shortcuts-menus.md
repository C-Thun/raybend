# M2-W3（阶段 1–5）：命令注册表 / 命令面板 / 快捷键体系 / 菜单兜底 / 键位迁移

完成时间：2026-09-20 05:38:56 CST

> 计划：`plans/M2-W3.md`（人类 2026-09-20 定：**不走 plannotator**，写完直接开干）。
> 本记录覆盖**阶段 1–5**；阶段 6（性能收尾：`perf:browse` / `perf:win` 脚本）见文末「遗留」。

## 交付了什么（人能看到什么）

| # | 能力 | 怎么用 |
| --- | --- | --- |
| 1 | **命令面板** | `Ctrl+K`；输入即模糊搜（中英文都行，也能搜命令 id）；每行右侧显示**当前键位**；空查询 = 最近使用；`↑↓` 选、`Enter` 跑、`Esc` 关 |
| 2 | **快捷键可自定义** | `Ctrl+,`（或 帮助菜单 → 快捷键设置）：点某行的键位 → 按下新键；`Backspace` 解绑、`Esc` 取消；冲突红字拦保存；**导入 / 导出 JSON**（要么全过要么不动）；全部恢复默认 |
| 3 | **菜单兜底** | 标题栏五个菜单（文件 / 编辑 / 视图 / 窗口 / 帮助），**每一项都来自命令注册表**（所以「菜单项同时是可搜索命令」是结构保证），右侧带键位提示，不可用的项暗着可见 |
| 4 | **键位统一走分发器** | 网格/看图的所有键（数字打星、`P/X/U`、`Delete`、`Ctrl+A`、`←→`、`i`、`Tab`、看图里的 `Esc`/`←→`/`+−01`、对比里的回车）都从「各自的 window 监听」搬进**一个**监听 → 所以改键是真的生效 |
| 5 | 顺带 | `Ctrl+1…4` 切工作流；命令面板里能搜到并执行「按时间」「排序」「密度」「主题」「窗口三键」…… |

## 代码结构（`plans/M2-W3.md` §2.2 的落地）

```text
src/lib/                 ← 纯逻辑（有单测，不认识 DOM / i18n / store）
  key-chords.ts            键位串解析 / 格式化 / 匹配（Mod 抽象、`+`=`、命名键别名、保留键）
  commands.ts              CommandSpec 类型 + 有效键位 + 冲突检测（作用域相交才冲突）
  command-match.ts         命令面板的模糊匹配（前缀 > 词首 > 子序列 + 最近使用加分）
  shortcuts.ts             偏好存储（localStorage）+ 最近使用 + 导入导出（结构化问题）
src/features/commands/   ← 一级模块（不认识别的 feature）
  catalog.ts               **全部命令**（≈60 条）+ `CommandDeps`（动作由组装层注入）
  dispatcher.ts            唯一 keydown 监听：匹配 → when/enabled → 执行
  CommandPalette.tsx       面板 UI
  ShortcutSettingsDialog.tsx  设置 UI（草稿 + 冲突检测 + 导入导出）
  messages.ts              冲突/导入问题 → 人话（判定在 lib、措辞在这里）
src/features/browse/actions.ts   ← 工作区的动作槽（删除确认 / 进看图 / Tab 三态 / 对比胶片带）
src/features/import/actions.ts   ← 同上（导入侧那一份）
src/components/ui/viewer/actions.ts ← 看图动作槽（Viewer 与 CompareView 各自注册实现）
src/features/browse/mark-actions.ts ← 标记动作**唯一实现**（工具条与命令面板共用）
```

组装层（`App.tsx`）把 store 动作灌进 `CommandDeps`，建注册表、挂分发器、挂两个浮层 ——
`features/` 之间**零 import**（`lint:arch` 全程绿）。

## 关键决策（都写进了代码注释）

1. **能力注入而不是全局单例**：命令不认识 store，`run()` 里调的是 `deps.xxx()`；
   这样 `features/commands/` 不 import 别的 feature，也就能用替身单测。
2. **作用域只有三个**：`global` / `tiles` / `viewer`。`tiles` 同时覆盖导入网格与浏览网格
   （键位语义本来一样），`viewer` 覆盖单张与对比（同一时刻只挂一个）。
   **冲突判据**：同键 + 作用域相交 = 拦；不相交 = 提示（`0`–`5` 在网格打星、在看图缩放就是这种）。
3. **`when()` 与 `enabled()` 分开**：前者「此刻适不适用」（决定接不接键、面板显不显示），
   后者「能不能执行」（菜单暗着、面板不可点）。踩过一个坑：`全选` 一度写成
   `enabled: 有选中` —— 那恰恰是没选中时才要用的功能，冒烟当场抓住。
4. **标记动作抽成一份**（`mark-actions.ts`）：筛选态改条件 / 标记态打标的分支、
   「被锁挡住」的提示，工具条与命令面板共用 —— 否则同一个动作会有两种行为。
5. **`Enter` 是内建**：网格里回车 = 进看图、看图里回车 = 返回，这是**焦点语义**不是快捷键，
   留在组件里；设置界面里有一段「内建键位」说明，免得用户以为漏了。
6. **检查 `shouldHandleKey`**：分发器不接输入框 / 模态 / Ark 组件内部的键。

## 验证

* `pnpm typecheck` 0 ｜ `pnpm test` **762**（新增 key-chords 13 / commands 11 / command-match 11 / shortcuts 12）
  ｜ `lint:colors` / `lint:arch` / `lint:i18n` ✓ ｜ `pnpm build` ✓ ｜ `pnpm smoke:ui` → `problems: []`；
* `pnpm check:browse` ✓，**本轮新增断言**：
  * `Ctrl+K` 开面板 → 有行、有键位（`信息档位` 显示 `I`）；
  * 搜「按时间」→ 回车 → 控制条上的「按时间」真的被切换，且面板自动关；
  * `Ctrl+,` 开设置 → 行数 ≥ 30；把「信息档位」改到 `Ctrl+Alt+I` → 落盘（localStorage 里能读到）
    → 新键生效、**旧键 `i` 不再触发** → 全部恢复默认；
  * 标题栏悬停 → 五个菜单都在；点开「编辑」→ 有「撤销」且带 `Ctrl+Z` 提示。
* **迁移回归**：W2 的全部键位断言（数字打星、`P/X/U`、`Delete`、`Ctrl+A`、`←→`、
  `i` 三态、`Tab` 三态、看图 `0/1/+−`、对比回车切胶片带）在迁移后**原样通过** ——
  这是本次最大的风险点，靠 `check:browse` 的既有断言兜住。
* 性能（阶段 6 的一半）：`cargo run -q --release -p raybend --example query-bench`
  → 10 万条合成库：3 星及以上 P95 **4.21ms**、红标 0.21ms、3星且红标 2.83ms、
  近 30 天 0.29ms、中文检索 3.68ms；最慢一条（短词 LIKE 兜底）P95 **28.29ms** ——
  全部远低于 100ms 判据。

## 遗留

1. **阶段 6 的脚本**：`scripts/perf-browse.mjs`（无头、假后端 10 万条：筛选感知延迟 +
   JS 帧预算 + 堆）与 `scripts/perf-win.mjs`（真机 2560×1440 采样）**尚未写**；
   `scripts/lib/cdp.mjs`（三个 CDP 脚本的公共部分）也还没抽。
   ⇒ 60fps 的裁决本来就归人类（`AGENTS.md` §2.8），这两条留到下一轮。
2. **`design/browse.pen` 的命令面板帧**未复核（需 Pencil，当前未连接）；菜单 / 快捷键设置
   两帧画布上没有 —— 需要人类开一次 Pencil。
3. 真机目视：面板/菜单/设置三处观感与键位手感。
