# 裁切重进与控制块修正
完成时间：2026-09-26 02:13:15 CST

## 范围与结果

- 重新进入已确认的裁切时，Rust 从保存的原图坐标框启动草稿，显影请求完整原图；同步相同比例不再把旧框重新居中。用户可以把八个把手向先前框外扩展。真正更换比例时仍重新求合法裁切框。
- 上次确认的比例模式及自定义宽高随该照片的 `EditGeometry` JSON 保存；换照片、撤销/重做重读栈、取消后再次进入时各自恢复。旧版记录没有比例字段时按自由比例打开，保持旧框。确认旋转不会擦除已有比例配置。
- 开启裁切的意图同时携带初始比例，避免开启工具和单独比例同步两条异步消息乱序时第一下拖动不锁比例。
- 宽高输入在 `input` 事件即时切入自定义；宽度输入的 Tab 顺序直接到高度输入。比例菜单按钮有可见的主题背景，宽高标签右对齐靠近输入框。两列布局及说明同步到 `design/editor.pen`、`design/editor.md`。
- 无新用户命令：仍使用现有的裁切命令、菜单与热键入口，比例/宽高是工具内部控件。

## 关键文件与决策

- `crates/raybend/src/develop/geometry.rs`、`crates/raybend/src/store/develop.rs`：可选的比例元数据放入现有几何 JSON，不变更数据库 schema；旧 JSON 反序列化兼容。比例元数据不进入预览缓存签名，也不参与图像像素计算。
- `src-tauri/src/editor.rs`：重进框保留、初始比例随工具开启、比例切换时按原有几何边界求新框。
- `src/features/editor/store.ts`、`src/workspaces/editor/EditorWorkspace.tsx`、`src/api/types.ts`：每张照片恢复最后确认的配置；确认时抓取同一时刻的比例草稿。
- `src/features/editor/panels.tsx`、`design/editor.pen`、`design/editor.md`：裁切控制块布局、输入与菜单视觉。

## 验证

- `pnpm test`：939 项通过。
- `cargo test -p raybend --lib`：1065 通过、2 忽略。
- `cargo test -p raybend-desktop --lib`：83 通过、1 忽略。
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm build`、`git diff --check` 均通过。
- 单元测试覆盖旧记录读取、所有比例项序列化、比例配置往返、缓存签名不变、取消重进、换照片隔离、异步意图字段以及从已确认框向原图外侧扩展。

## 遗留与人工验证

- Windows 版仍有运行中的 `raybend-desktop.exe`，当前 exe 早于本次 `dist/`，不能覆盖运行中的二进制。关闭程序后需执行 `pnpm debug:win` 重建并按项目流程 `pnpm check:win` 核对。
- GUI 的裁切框视觉位置、把手跟手程度、重进及扩大范围需人类在 Windows 真机 E2E 验证；Agent 只完成编译和单元冒烟。
