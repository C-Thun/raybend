# 导出小片区：RAW 收回弹窗 · 入口按钮改名 · 库探测收敛 · 小片名字条配色修复

> 完成时间：**2026-10-11 03:40**（同一天第二轮；上一轮见 `2026-10-11_toolsbar-compact-export-filter-raw.md`）
> 需求：崔总 2026-10-11 口述四条（RAW 收回弹窗 / 全部定稿入口改名 / 小片名字条浅色模式看不见 / 库被反复探测）
> 画稿与规格：`design/export.md` §4.2 与 2026-10-11 修订节、`specs/toolsbar-compact-and-export-gallery.md` §1.5（第二轮）、`specs/export-issue-ordinal.md` §4

---

## 1. 导出 tile 的小片区（`ExportWorkspace.IssueStrip`）

| 改动 | 前 | 后 |
| --- | --- | --- |
| RAW 的位置 | 右端一个 `RAW` **按钮**（直接入队） | **取消按钮**；RAW 回到「像 SOOC 一样的小片」，但**只住在「全部定稿」弹窗里** |
| 入口按钮 | `全部定稿`（多于两片才出）+ 右侧 `RAW` | **一个按钮**，名字按「弹窗比 tile 多出什么」定 |
| 光标 | 纯 `<button>`（Tailwind v4 preflight 下是箭头光标） | 走共享 `Button`（ghost）：**指向手形** + 聚焦环，外观仍是纯文字 |
| 底部 | 无留白，入口文字贴着下一行的大图 | 容器加 `pb-1`（4px），`issueExtraHeight` 同步 `… + 24 + 4` |

**入口按钮的取名规则**（纯函数 `lib/export-model.ts::issueStrip` 一次算完，组件不再自己判断）：

| 名字 | 条件 |
| --- | --- |
| `全部定稿` | 还有非 RAW 的定稿没摆下（`hidden > 0`） |
| `RAW` | 没别的可看、**只多一张 RAW**（`hidden === 0 && raw !== null`） |
| 不出按钮 | 弹窗里没有任何比 tile 更多的内容（`entry === null`） |

实现上 `IssueStrip` 的返回从 `{tiles, more, raw}` 改成 `{tiles, hidden, raw, entry}`；
`source.ts::issueExtraHeight({tiles, entry}, cellSize)` 与它同改同测（`source.test.ts` / `export-model.test.ts`）。

**RAW 片的图不直读 RAW**（崔总：直读太贵）：`IssueTile` 对**非主行的** raw 变体改借同一张照片的
**SOOC** 小图（`getVariantThumb` 走同一条缓存键 —— 两片共用一个请求），再用既有「RAW 模拟」滤镜
（`Tile.approximate` → `.photo-raw-approximate`，`brightness .72 / saturate .35`）显示成没修过的原片样子。
零额外解码。RAW 单文件的照片主行就是它自己（没有 SOOC 可借）→ 保持原样。

**弹窗**：去掉 `filter(v => v.reference.variant !== "raw")`，RAW 因此回到「全部定稿」，
且因 `orderedIssues` 的 rank 固定排在**末尾**；选中与否位置不变，也永远不会冒到 tile 小片区
（`issueStrip` 恒把 RAW 从 `tiles` 里剔掉）。上一轮为「RAW 按钮」加的 `rawFor` / `canEnqueueRaw` /
`enqueueRaw` 三个 store 动作与 i18n `export.rawQueue` 一并删除（它现在就是一张普通的可选片）。

## 2. 小片名字条在浅色模式下看不见（`components/ui/Tile.tsx`）

- 现象：导出画廊的 issue 小片底部名字条在**浅色**下是深底 + 深字。
- 根因：`minimal` 那一档短条把**弹窗遮罩** `--scrim` 当成了信息条底纹。`--scrim` 是给弹窗背景压暗用的，
  浅色主题下它是**深色**（`fg-1` 42%），而条上的文字按 §12.9 规矩是 `fg-1` → 深底深字。
- 修法：换成 §12.9 定的 **tile 信息条中性蒙层** `--tile-bar-scrim`（深色 65% 压暗 / 浅色 72% 提亮），
  与 `text-fg-1` 配套 ⇒ 浅色「提亮条 + 深字」、深色「压暗条 + 浅字」，与大图信息条同一套。
- 规则写进 `memory/DESIGN.md` §12.9（补一条「`minimal` 是同一个信息条」）+ 修订日志。

## 3. 库探测收敛（`features/repositories/monitor.ts` + `App.tsx`）

崔总报：离线库在启动后被反复探测刷日志。收敛成**只有两类触发**：

| 触发 | 说明 |
| --- | --- |
| 启动一次 | `monitor.start()` 里 `request()` 把登记过的库各探一次 |
| 用户点那个库 | 换选中（导入列表 / 浏览·导出的库选择）→ `App.tsx` 的 effect 只认**换了库**；卡片的「重新查找」、命令「重新连接」、库设置里的重连直接走 `repositories.remount` |

删掉的三类触发（都是这次刷屏的来源）：
1. **自动重试链** `RETRY_MS = [1s, 3s, 10s]` —— 探完仍离线就停在离线，要再试由用户点；
2. **窗口回焦点** `window.focus → request()`（一次焦点就探全部库）；
3. **卷表变化** → `request()`（卷枚举仍每 2 秒跑，但只刷新盘符列表与导出唤醒，不再顺带探测）。

`probe()` 里的 `onRecovery` 逻辑保留（订阅失败时仍能报告恢复）；`monitor.test.ts` 七个用例按新契约重写
（「探一次就完、暂忙也不排下一轮、卷变化不探、released 不参加」）。
口径写回 `memory/FUNCTION-REPOSITORY.md` §2.3 与 §2.1。

## 4. 冒烟脚本

| 脚本 | 改动 |
| --- | --- |
| `check-export-boot.mjs` | RAW 断言改为：小片区**永不含** raw 片、入口按钮文字是 `RAW`、**光标必须是 `pointer`**；弹窗断言 10 → **11 片**（含 RAW）且 **RAW 必须是最后一个** |
| `check-browse-boot.mjs` | 夹具补 `browse_recycle_summary`、`browse_delete.deleted → moved`（回收站改造后的字段名）；`Ctrl+,` 那一步改成「先开**统一设置**，再点「快捷键」那一页」（现状口径，`help.shortcuts` 的默认键早已转给 `settings.open`）；异常断言连栈一起打 |
| `ui-smoke.mjs` | 整理三入口的按钮查询限定到 `[data-toolsbar-left]`（中段的「标签」图标化后也带无障碍名，不加作用域会点错） |

## 5. 验证（Agent，冒烟级别）

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | ✅ |
| `pnpm test` | ✅ 1267/1267 |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | ✅ |
| `pnpm build` | ✅ |
| `pnpm check:export` | ✅ |
| `pnpm check:browse` | ✅（本轮修掉全部 5 项：夹具 2 项、`Ctrl+,` 口径 1 项、异常 1 项、重读库 1 项） |
| `pnpm smoke:ui` | ✅ `problems: []` |

**未经人类验证**：浅色/深色下小片名字条的实际观感、入口按钮的手形手感、`RAW` 命名在小片多于两片时的
可读性、真机上插拔盘后再点库的重连体验（本次把自动重试去掉了，**慢盘启动那一下失败就要靠用户点一次**，
这是崔总要的口径，但值得真机确认一次）。

## 6. 遗留问题

1. 队列行/拖动预览若显示 RAW 行，仍走 `getVariantThumb` 的 raw 分支（真图缓存命中则用真图，否则内嵌预览）；
   本轮只把小片的图接到 SOOC 上。若也嫌贵，下一轮一并改。
2. 卷枚举每 2 秒一次仍在跑（给盘符列表与导出唤醒用）；若真机上看还有噪声，可一并降频。
