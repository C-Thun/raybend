# 编辑工具栏源与历史操作移到按钮组最左侧
完成时间：2026-09-25 18:16:08 CST

## 改动

- 将 toolsbar mid 中同一组按钮的顺序改为 SOOC/RAW → 撤销 → 重做 → 裁切 → 旋转 → 对比；整组仍按工具栏全宽居中，left/right 结构不变。
- 同步更新 `src/features/editor/toolbar.tsx`、`design/editor.md`，并在 Pencil 的 `design/editor.pen` 画布上调整主稿和组件稿两处按钮顺序。Pencil 画布改动尚未由工具写回磁盘（见上轮记录）。
- 仅调整现有动作的视觉顺序，没有新增命令或热键。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：921/921 通过。
- `git diff --check`：通过。
- `pnpm debug:win` 已完成前端构建，但 Windows cargo 替换 exe 时遇到 `os error 5`。重试时脚本查到 PID 23592 的 RayBend 窗口仍开着，因此停止以免关闭用户应用。当前 Windows exe 仍是旧按钮顺序，待窗口关闭后重建。

## 未经人类验证

- Windows GUI 按钮实际排列。待新 exe 构建后由人类 E2E 确认。
