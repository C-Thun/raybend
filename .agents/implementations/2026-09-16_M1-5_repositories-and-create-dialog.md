# M1-5 C 批：右列库区、建库弹窗、导入动作区

完成时间：2026-09-16 05:16:18 CST

## 本次改动的范围

导入工作区**右列**：库列表（含在线/离线与多路径形态）、建库弹窗（含目录选择器）、
底部动作区（选中统计 + 导入按钮可用性 + 避免重复导入）；外加画布补帧与冒烟断言补齐。

## 涉及文件

#### 画布与文档

| 文件 | 内容 |
| --- | --- |
| `design/main.pen` | 新增两帧：`Dialog / 新建库`（`HFzJZ`）、`Dialog / 新建库 · 错误态`（`sccht`） |
| `design/main.md` | §3.4 弹窗说明（三种判定行）+ §9.5「M1-5 有意不做、需要记一笔的」 |

#### 前端

| 文件 | 内容 |
| --- | --- |
| `src/api/dialog.ts` + `dialog.test.ts` | 目录选择器封装：浏览器里**返回 null 而不是抛错**（退路是手打路径） |
| `src/features/repositories/create-logic.ts` + `create-logic.test.ts` | 建库分支判定（纯函数，8 项单测） |
| `src/features/repositories/RepositoryList.tsx` | 库卡片 + 离线重挂载 + 多路径条数 + 空态 + 「+ 新建库」入口 |
| `src/features/repositories/CreateRepositoryDialog.tsx` | 弹窗：探测（防抖 250ms）→ 判定行 → 提交 |
| `src/features/repositories/RepositoryFooter.tsx` | 选中统计 + 导入按钮 + 避免重复导入 |
| `src/workspaces/import/{ImportWorkspace.tsx,store.ts}` | 右列接线；store 增 `remount/remountErrors/avoidDuplicates/hydratePreferences` |
| `src/i18n/{zh-CN,en-US}.ts` | 21 条新文案（两侧同步） |

#### 外壳与脚本

| 文件 | 内容 |
| --- | --- |
| `src-tauri/{Cargo.toml,src/lib.rs}` | 接入 `tauri-plugin-dialog` 2.7.3 并注册 |
| `src-tauri/capabilities/default.json` | 加 `dialog:default` 权限 |
| `scripts/ui-smoke.mjs` | 新增导入工作区断言；控制台收集抽成函数；**脚本自己导航到应用外壳** |

## 关键决策与理由

1. **目录选择器用官方插件，失败可退回手打路径**（计划里已评审）：
   `tauri-plugin-dialog` 是 Tauri 官方仓库的成员，Windows 上走原生对话框。
   前端的 `pickDirectory()` 在浏览器里返回 `null` 并提示「手动填写路径即可」——
   开发预览不该因为缺少原生能力而把弹窗弄崩。
2. **分支判定抽成纯函数 `evaluateCreate`**（`REPOSITORY.md` §2.4）：三种结局 —— 新建 / 登记已有库 / 拒绝。
   两条容易做错的规则被单测钉住：
   * 探测还没回来（`probe === null`）时**允许提交** —— Rust 侧建库前会再探一次，
     在这里拦住用户只会让人以为卡住了；
   * 改了路径后旧探测结果立刻作废（`probeMatches`），否则「已有库」的提示会挂在另一个路径上。
3. **错误态用同一个 info 图标换色**，不换成警告三角（画布上就是这个口径）：
   三种提示是同一句话的三种语气，换图标会让人以为是不同性质的问题。
4. **照片数读不到时显示「—」而不是 0**：0 会让用户以为库是空的。
5. **离线不是错误**（`REPOSITORY.md` §2.3）：离线徽标可点 = 对所有登记路径重查一次，
   找不到只在卡片上记一句「已试过 N 处已登记路径」，颜色用次级色、不用 `danger`。
6. **导入按钮的可用性是左右两边的合取**：左边至少勾选一个目录 **且** 右边选中了一个**在线**的库。
   禁用时必须给一句原因（`先在左边勾选目录、再选一个库`）—— 禁用而不说原因的按钮等于谜题。
7. **「避免重复导入」默认开**（`REPOSITORY.md` §4.3），状态写回 `app.db` 的设置表跨会话记住。
8. **导入按钮本单元只做到「可点」**：真正的执行、模版求值、进度弹窗属 M1-6；
   点了之后显示一句「导入中…（M1-6 实现）」，**明说没实现**而不是静默无反应。

## 验证方式

* `cargo test -p raybend`：399 项全绿；`cargo clippy --workspace --all-targets`：0 警告
* `cargo test -p raybend-desktop`：15 项（含 DTO 契约夹具）
* `pnpm test`：356 项（新增 11 项：`create-logic` 8 + `dialog` 1 + store 的 `remount/偏好` 2）
* `pnpm typecheck / lint:arch / lint:colors / build`：全绿
* **Windows 侧**：`cmd.exe` + `CARGO_TARGET_DIR=C:\rb-target\raybend` 跑 `cargo check --workspace --all-targets`
  —— 0 警告 0 错误（`tauri-plugin-dialog` 在 Windows 目标上编译通过）
* `pnpm smoke:ui`：新增断言实测结果 —— 右列宽 340px、空态文案在、导入按钮 `disabled=true` 且给出原因、
  点「+ 新建库」弹窗真的打开（`role=dialog`，含名称/库根目录字段、空路径时提交禁用且无判定行）、点「取消」能关上

## 遗留问题

* **齿轮（库设置）按计划不在本单元**：画布上有、实现里没有，属 M1-6（已记 `design/main.md` §9.5，避免静默漂移）。
* 三列的空态与缩略图骨架态画布上没有对应帧，实现按现有面色做了朴素版；若与设计语言不符，M1-6 收尾补帧。
* 建库/重挂载的**真实磁盘行为**（跨卷挂载、只读目录、UNICODE 路径）需要在 Windows 侧用真实目录确认 —— 归人类验收。
