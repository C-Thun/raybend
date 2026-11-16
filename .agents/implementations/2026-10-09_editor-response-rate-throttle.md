完成时间：2026-10-09 13:03:10 +0800

# 编辑调节响应率：两档时间限流 + 设置页「编辑」页

## 需求（崔总 2026-10-09）

> 检查 editor 中拉面板的控制杆时的防抖现在是多少时间？哦对了这里应该是限流不是防抖，
> 我怎么感觉延迟大到像根本不支持实时预览一样。这样吧，在配置窗口中增加一个合适的位置，
> 用于开关编辑模式下的调节响应率，现在就分高和低 2 档，选低档的话 0.5 秒限流，
> 选高的话 0.1 秒限流，松开鼠标时必然触发一次计算。

## 查明的现状（先回答「现在是多少」）

**前端根本没有防抖/限流**。拉杆 → `paramsSender.push()`（`createLatestCoalescer`，**帧级合并**：
同一帧只发最后一条 + 尾样本必发）→ `editor_set_params` → Rust 渲染线程按「最新者优先」丢弃过期任务。
所以拖动时的画面更新率 = **后端重算耗时**（大图上几百毫秒→感觉「不实时」）；
前端这边没有额外的时间闸。

## 实现

* **`src/lib/editor-intent.ts`**：给 `createLatestCoalescer` 加 `intervalMs?: () => number`（时间限流）
  —— 帧合并与时间限流叠乘：每帧最多一条 **且** 窗口内最多一条；窗口内只留最新值、到点补发；
  `flush()` **无视限流**（松手必发）；`dispose()` 清定时器。缺省 `intervalMs` 不传 = 行为与原来完全一致
  （其它调用点不受影响）。定时器句柄用 `ReturnType<typeof setTimeout>`（DOM/Node 两边都对）。
* **`src/lib/editor-response.ts`（新）**：设备级偏好（`localStorage`，`raybend.editor.response_rate`）——
  高档 100ms（默认）/ 低档 500ms；非法值回落、存储抛错静默降级，配 4 项单测。
* **`EditorWorkspace.tsx`**：`paramsSender` 的 `intervalMs` 读偏好；`commitDevelop()` 开头
  `paramsSender.flush()` —— 落库前先把挂起的参数发出去（松开鼠标必然触发一次计算）。
* **设置页**：`GlobalSettingsDialog` 新增「编辑」页（`SettingsPage` 加 `"editor"`，图标 `IconAdjustments`），
  一项「调节响应率」两档分段控件；解释走标题的**原生 tooltip**（`hint`），不占版面（`AGENTS.md` §2.20）。
* **组装层**：`App.tsx` 创建 `editorResponse` store（设置页写、编辑器读，所以住在组装层），
  传给 `EditorWorkspace` 与 `GlobalSettingsDialog`。

## 验证（Agent 冒烟）

* `src/lib/editor-intent.test.ts` 新增「时间限流：窗口内只发最新值，到点补发，松手无视限流必发」
  与「不限流时仍是帧级行为」两条（注入假时钟/假定时器，确定性推进）；
* `src/lib/editor-response.test.ts` 4 项（默认档、非法回落、写回读取、存储抛错降级）；
* 定向跑 19/19 通过；`pnpm typecheck`、全量 `pnpm test`（1239）通过。

## 遗留

* 限流只压**拖动期间**；真机上是否「跟手」仍取决于后端重算速度 —— 若高档仍嫌慢，
  下一步可做「拖动中降分辨率预览」（未做，属另一件事）。
* 真机验收：两档的手感、设置页开关是否生效（档位改动**下一次 push 生效**，无需重进编辑器）。
