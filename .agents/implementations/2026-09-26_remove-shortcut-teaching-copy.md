# 移除产品界面的快捷操作教学说明
完成时间：2026-09-26 14:26:00 CST

## 改动与理由

崔总明确：快捷操作用于保持界面干净、节省空间并保留能力，发现与教学放在软件外；不能因为操作隐藏，就把只需读一次的教学文字堆进产品界面。

- 移除共用 `EasyDestroyButton` / `EasyDestroyHost` 自动附加的 Shift 教学文案，覆盖所有使用该组件的移除场景；不恢复此前的隐藏辅助教学文本。
- LUT 移除按钮的标题 / 无障碍名称只保留动作名称，定稿删除按钮的 tooltip 同样只保留动作名称。
- 删除两种语言中已无调用的 `common.easy_destroy.shift_hint`，没有保留闲置提示入口。
- 普通点击确认、Shift 快通道及取消行为沿用已有控制器；确认框只说明当前操作对象与后果。
- 沿用 pencil-design / frontend-design，使用 Pencil MCP 更新 editor.pen 的定稿 / LUT 确认框（`aPhpa`、`d7qOt`、`ByjFD`），删除教学段落；截图与结构检查无溢出。editor.md 同步。
- 将软件外教学原则写入 DESIGN.md §12.2，覆盖正文、tooltip、常驻提示和隐藏辅助文案，供后续实现遵循。上一份实施记录是当时的历史状态，本记录明确取代其中“正文可见 Shift 提示”的选择。

涉及文件：`src/components/ui/EasyDestroy.tsx`、`Dialog.tsx`、`src/features/editor/lut-panel.tsx`、`panels.tsx`、`src/features/browse/BrowseToolbar.tsx`（过时注释）、`src/i18n/zh-CN.ts`、`en-US.ts`、`design/editor.pen` / `editor.md`、`DESIGN.md`。

## 验证：冒烟

- `pnpm typecheck`：通过。
- `pnpm test`：958 通过，0 失败；已有 easy destroy 行为回归与语言包一致性检查均通过。
- `pnpm lint:colors`、`lint:arch`、`lint:i18n`：通过。
- `git diff --check`：相关文本文件通过。
- 定位检查确认生产组件和语言包不再引用 `common.easy_destroy.shift_hint`。
- `pnpm debug:win`：通过，含前端生产构建和 `pnpm check:win`。嵌入的 4 个资源全部匹配，worker 仍含当前协议 tag。Windows debug exe 更新于 **2026-09-26 14:25:17 CST**。

产物：`C:\rb-target\raybend\debug\raybend-desktop.exe`。

本轮只是移除教学文案，没有新增模块或动作，复用已有控制器及其单元测试；没有新增命令或热键。真机交互与视觉仍由崔总确认，未声称完成产品 E2E。
