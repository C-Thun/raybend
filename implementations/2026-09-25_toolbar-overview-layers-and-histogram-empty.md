# 工具栏、全图总览、浮层顺序与直方图空态
完成时间：2026-09-25 18:02:49 CST

## 范围

- `ToolsBar` 保持 left / mid / right 三层：mid 宽度永远为整条工具栏，工具作为一组按整窗中心对齐；left/right 盖在 mid 上，内侧渐隐。编辑器 mid 按裁切、旋转、对比、SOOC/RAW、撤销、重做排成同一行。取消 mid 内的二次区域。
- browse 与 editor 的总览共用 `PreviewFrame`：右栏宽度不变，框的宽高比随照片变化，限定在 3:1～3:4；超出比例时完整图片居中留白。总览只用完整 Screen 图，最长边不超过 1920，不拿会裁切的 grid/strip 图冒充。
- 浮层层级调整为 message > easy copy tip > 弹窗 > titlebar > 弹窗遮罩。easy copy 的触发元素仍在原层，遮罩阻止交互；只把 Portal 气泡提升。Dialog 的全屏 positioner 不再截获 titlebar 指针事件。导入取消确认与迁移遮罩沿用这套层级。
- `Histogram` 自己处理无数据状态：保持 176px 高度、网格与通道行；峰状图为空时在绘图区中央显示「暂无数据」。`HistogramPanel` 无论数据是否到达都渲染同一组件，避免高度跳变。
- 通过 Pencil MCP 在 `design/editor.pen` 和 `design/browse.pen` 的画布中更新工具栏、总览说明及直方图空态示例，并同步更新 `design/editor.md`、`design/browse.md`、`DESIGN.md`、`BROWSE.md`、`IMAGING.md` 的规则；`AGENTS.md` 的工具栏术语也对齐本轮决定。Pencil MCP 的画布修改未写回本地 `.pen` 文件，见遗留问题。

## 关键决策

- Screen 档后端已有完整等比输出与长边 1920 上限，复用现有 `getViewImage(..., "screen")`；只在总览入口改图源，不增加第二套图片生成逻辑。
- 视野框百分比按实际图片盒计算，外框留白不参与坐标，避免极端比例照片的视野框偏移。
- 直方图空数据使用已有主题的 `fg-2`，深浅主题均可读；空态仍保留通道按钮和固定尺寸。
- 本轮没有新增用户命令。工具栏仅重排现有动作，总览与直方图属于显示逻辑，浮层层级也不适合进入命令面板。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：921/921 通过，涵盖总览比例边界、Screen 与 grid 分离、z 轴顺序及直方图空数据判定。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm build`、`git diff --check`：通过。
- `pnpm debug:win`：完成 Windows debug 构建；`check:win` 确认 exe 嵌入最新前端资源且 worker 协议为 v3。产物：`/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`。
- Pencil 画布新增编辑与浏览的直方图空态帧后检查布局，无剪裁或溢出问题；磁盘文件状态检查表明新画布操作未持久化。

## 未经人类验证

- Windows WebView2 的工具栏遮挡/居中、拖动窗口时的层级与直方图视觉体感；这些属于真机 GUI E2E，需人类检查。
- 双层导入取消对话框的交互仍需 Windows GUI 确认；代码层级与类型检查已通过。

## 遗留问题

- Pencil MCP 返回修改成功且画布可见，但 `design/editor.pen` 的磁盘时间未随本轮改动更新，`design/browse.pen` 在 Git 中仍未修改。尝试用 Computer Use 保存时，Windows UI 工具因当前 WSL 工作目录的 `sandboxCwd is not a local file URI` 错误无法初始化。需要在 Pencil 中把这两个打开的设计画布保存到磁盘；说明文档与实现代码已经落盘。
