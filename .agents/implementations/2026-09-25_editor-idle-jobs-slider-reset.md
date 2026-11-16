# 编辑器空闲显影与拉杆手柄复原
完成时间：2026-09-25 16:23:14 CST

## 范围
- `src/features/editor/store.ts`：状态轮询读到相同 as-shot 色温时保留参数对象身份；真实变化的色温信号与参数写入用 Solid `batch` 合成一次响应式更新，避免每 250ms 重发 Preview 显影 job。
- `src/features/editor/SliderRow.tsx`、`panels.tsx`：共用参数拉杆的把手支持双击复原；色温复原到拍摄基准值，镜头参数复原到相对默认值，旋转复原到 0。只点把手而未改值时不提交。
- `src/features/editor/toolbar.tsx`、`src/App.tsx`、`src/shell/ToolsBar.tsx`：LUT 保持在 toolsbar left，SOOC/RAW 与撤销/重做位于 toolsbar center 最左，画布工具位于中段剩余区域的中央；统一重置仍在 right。
- `design/editor.pen`、`design/editor.md`、`DESIGN.md`：同步组件及主稿布局和拉杆交互。
- `src/features/editor/store.test.ts`：增加色温相同值保留对象身份，以及静态/镜头微调/旋转拉杆复原到零点的回归断言。

## 决策
- 根因在 `setAsShotTemperature`：`EditorWorkspace` 的 250ms 状态轮询会反复调用它，原逻辑每次都生成新参数对象，即使色温没变；显影载荷响应式 effect 因此不停调用 `editor_set_params`。修复实际重复提交，保留真实变化后的显影。
- 双击把手是控件内交互，复用既有 `resetParam` / `resetAngle`，不新增命令或默认热键；SOOC/RAW、撤销/重做和全局重置沿用既有命令。

## 验证
- `pnpm typecheck`、`pnpm test`（919/919）、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm lint:colors`、`pnpm build` 通过。
- 使用 Node `--conditions=browser` 的 Solid 响应式运行时冒烟：首次实际色温更新触发一次载荷计算，重复相同色温 0 次，换成新色温再触发一次。
- `git diff --check` 通过。Windows 调试版构建预检发现用户仍在运行的 `raybend-desktop.exe`（PID 30524），脚本按项目规则未终止应用，也未覆盖被占用的 exe；Windows 产物待窗口关闭后重建。
- `/mnt/c/src/tmp/rb-editor.log` 当前大小为 0 字节，不能据此分析此前崩溃；这轮定位依据代码触发链和响应式冒烟。

## 遗留与人工验证
- Windows exe 需要在用户关闭正在运行的应用后执行 `pnpm debug:win` 与 `pnpm check:win`。
- 双击把手、工具栏位置、空闲十秒日志是否停止增长属于真实 Windows GUI E2E，需人类在新版本上确认；Agent 尚未验证这些视觉与体感结果。
