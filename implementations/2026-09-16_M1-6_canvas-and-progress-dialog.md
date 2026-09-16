# M1-6 C 批（部分）：画布五帧 + 导入进度弹窗接线

完成时间：2026-09-16 12:14:03 CST

## 本次改动的范围

`plans/M1-6.md` 步骤 11（画布五帧）与步骤 12（进度弹窗 + 与导入按钮接线）。
库设置弹窗（13）、导入后刷新与「在库中查看」（14）、smoke 断言与真实样本闭环（15 的一半）
**仍未完成**，见文末。

## 涉及文件

#### 画布（`design/main.pen`）

| 帧 | id | 内容 |
| --- | --- | --- |
| `Dialog / 导入进度` | `st3Fs` | 阶段条（扫描 › 规划 › 导入 › 缩略图）、进度条 68%、计数（已导入 82 / 跳过 3 / 失败 1）、当前项（文件 + 库内目标路径）、取消 / 暂停 |
| `Dialog / 导入进度 · 错误清单` | `NPFmL` | 同上 + 展开的错误列表（文件名 + 原因，红色）、导出清单按钮 |
| `Dialog / 导入进度 · 结束` | `oS1NL` | 导入完成 + 摘要行 + 计数 + 导出错误清单 / 关闭 / 在库中查看这些照片 |
| `Dialog / 库设置` | `A9vPG` | 导入模版输入框、8 个变量 chip、三条落盘预览路径 |
| `Dialog / 关于 About` | `FPU9M` | 版本 · 许可（AGPL-3.0-only）· 第三方声明 · 「照片永远不会被写回原文件」 |

五帧都用与 `Dialog / 新建库` 同一套令牌与结构（`$surface-layer` / `$radius` / 16 内边距 /
12 间距 / 32 高按钮），所以看上去是同一个应用里的东西。

#### 前端

* `src/api/dialog.ts`：新增 `pickSaveFile`（导出错误清单用；浏览器里返回 `null`）。
* `src/features/import/ImportProgressDialog.tsx`：进度弹窗（阶段条 / 进度条 / 计数 /
  当前项 / 错误清单 / 暂停·继续·取消 / 结束态），取消要二次确认并把
  「已导入的会保留」写在确认里。
* `src/features/import/index.ts`：模块出口。
* `src/workspaces/import/ImportWorkspace.tsx`：把导入按钮接到真流程 ——
  预检（空间偏紧就在右列问一句）→ `begin()` → 弹窗；`db.importPrecheck/importStart`
  之类的调用改到 `api/import.ts`。
* `src/api/import.ts` + `src/features/import/store.ts` + `store.test.ts`：
  `import_start` / `import_precheck` 的参数从「一个布尔值」改成**每个源目录带自己的
  「包含子目录」开关**（界面上的来源树本来就是每目录一份，拿一个布尔值糊弄整批是错的）。
* i18n：两侧各加 25 条（`import.*` 与 `common.close`）。

## 关键决策与理由

1. **画布先画后写**（`AGENTS.md` §5.1）：五帧都在弹窗代码之前落到了 `main.pen`。
   顺带发现 pen.dev 的 `Copy` 只能对**组件实例**用 `descendants` 覆盖文字 ——
   普通帧的复制必须 `Get` 出新 id 再 `Update`（第一版帧因此是空的，重建后才对）。
2. **进度条只在导入阶段给确定值**：扫描/规划没有总数，`percent()` 返回 `null`，
   界面渲染成空的轨道（不确定态）。硬凑一个百分比会让进度条先快后慢。
3. **取消 = 二次确认 + 明说保留**：确认框里写清「已经导入的照片会保留在库里，取消不会回滚」——
   这句话是 `REPOSITORY.md` §4.5 的界面表达，不能只在文档里。
4. **每目录一个「包含子目录」开关**：改的是命令签名（不是前端糊一下），
   因为 `RunRequest` 本来就是每目录一个，前端假装成整批一个只会让行为对不上界面。
5. **空间偏紧就地问一句**，不套第二层弹窗：右列一行红字 + 「仍然继续」。
   预检失败（网络盘读不到可用空间）不拦人。

## 验证方式

* `pnpm typecheck` / `pnpm test`（373 项）/ `pnpm build`：全绿。
* `cargo test -p raybend-desktop`（19 项，含契约断言）/ `cargo clippy --workspace --all-targets`：0 警告。
* `pnpm smoke:ui`：`problems: []`（右列几何、导入按钮禁用与原因、建库弹窗开关仍全过）。
* 画布：五帧各截图确认过（阶段条/进度条/计数/当前项/错误列表/按钮都在，无塌陷与溢出）。

## 遗留问题（未完成，按 `plans/M1-6.md` 的编号）

1. **步骤 13 库设置弹窗**没写：画布帧（`A9vPG`）有了，`features/repositories/LibrarySettingsDialog.tsx`
   与「齿轮」接线还没做；`design/main.md` §9.5 的待同步清单里仍记着「齿轮 M1-5 不渲染」。
2. **步骤 14 导入后刷新与「在库中查看」**没接：弹窗上的该按钮当前**不显示**
   （`onRevealInLibrary` 没传），因为切工作流要动 `App.tsx` 的 flow 状态；库卡片张数与网格刷新同理。
3. **步骤 15 的 smoke 断言**没加：`scripts/ui-smoke.mjs` 还没有针对进度弹窗的断言。
4. **真实样本闭环没跑**：`plans/M1-6.md` §6 的「20~30 张真实照片导入 → 落盘断言」
   （模版路径、`_RAW/` 分流、序号、`import_items` 状态、缩略图入队）**一次都还没在真磁盘上跑**；
   目前只有内存实现（134 项）与临时 `catalog.db`（14 项）的保证。
   这是本单元最该补的一步，也是「`ASSISTANCE.md` 人类验收」里那几项的前提。
5. **跑完整批的真实耗时/事件频率**没测（`plans/M1-6.md` §6 的规模项）。
