# M3-W5 工具拖动与裁切面板修正
完成时间：2026-09-26 00:56:10 CST

## 范围与原因

- 旧版裁切/旋转框、对比分线依赖 `editor_render_state` 的 250ms 轮询，只能每秒更新四次；原生照片已改变而 DOM 线还停在旧位置。工具指针逐条 IPC，且每次都克隆/序列化调用方根本不用的整份状态，包含直方图。旋转拉线只有按下起点和松手计算，移动阶段完全没有绘制。
- 进入裁切/旋转后右栏保留旧滚动位置；自定义比例只有被自动切入时才出现在菜单；宽高输入与比例控制没有对齐。

## 实现与复用

- `render/overlay.rs` / `overlay.wgsl`：小型矢量顶点管线，裁切框、三分网格、八把手、四块框外遮罩、旋转拉直线、对比分线和把手与照片在同一个 render pass / present 绘制。复用 `Viewport::frame_rect_css` / `compare_scissors`，不另写视口变换；分线直接取左右图像实际使用的 scissor 边界。每帧只上传少量顶点，不回读照片，不重新显影像素。
- `render/gpu.rs`：覆盖层资源纳入既有 `build_device_resources`，设备恢复随其它资源整套重建；对比松手也应用最终指针坐标。
- `src-tauri/src/editor.rs`：每次绘制从同一工具草稿构造覆盖层；旋转保存移动中的线终点，松手按原有算法摆正、取消清除。高频意图只回成功/错误，状态轮询保留握手与诊断用途。
- `src/lib/editor-intent.ts` / `viewport.tsx`：复用现成 `createLatestCoalescer` 合并工具指针，每帧最新移动；手势边界直接发，up 包含最后位置，取消/换工具/卸载清掉待发移动。移除旧 DOM 线及超大 box-shadow，仅保留未来图像像素覆盖物的通用 `ViewportOverlay` 宿主。
- `editor-viewport.ts`：随既有洞口事实上报四个主题颜色字符串；读取现有 CSS 令牌，Rust 复用 `Srgb8` 解析与线性化，未复制配色常量。
- `EditorWorkspace.tsx`：旋转角度也用帧合并，确认前 flush；消费退出期间的拉直修订，避免旧轮询快照覆盖刚进入工具的角度。
- `panels.tsx` / `store.ts` / `params.ts`：进入裁切或旋转立即回顶；裁切使用等宽两列、两行对齐，标签与宽高输入同行，取消/确认保留。自定义始终可选；宽高是草稿，选中自定义才生效；支持小数和正数，比例限定 1:100–100:1，非法输入恢复最近有效值。
- 先通过 Pencil MCP 修改现有 `design/editor.pen` 的裁切控制块（复用已有下拉、输入和按钮），截取控制块与整屏检查；同步同名 `design/editor.md`。

## 命令与热键

本次是既有功能修复，沿用统一命令 `editor.tool.crop` / `rotate` / `compare`，默认键 C / R / B 不变。比例输入、交换、解除限制与确认/取消属于工具内部控制，不新建全局命令或额外热键监听。

## 已验证（冒烟与单元测试）

- `pnpm test`：938 通过，新增指针帧合并/尾样本/取消/换工具/卸载、自定义比例与非法输入、主题载荷快照测试。
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm build` 通过；生产构建保留现有的大 chunk 提示。
- `cargo check -p raybend -p raybend-desktop` 通过。
- `cargo test -p raybend --lib`：1063 通过、2 个既有手动测试忽略；`cargo test -p raybend-desktop --lib`：81 通过、1 个既有手动测试忽略。
- 新增覆盖层几何测试：空洞口、多个 DPR、对比两端和中间位置精确采用 scissor 边界、旋转框与拉直线顶点。
- `cargo test -p raybend --lib tool_overlay_draws -- --nocapture`：实际 GPU shader/顶点布局/混合绘制与小图回读通过，0.17 秒，未跳过；断言移动后新位置为线色、旧位置恢复底色。这是合成图冒烟，不是真机 GUI/E2E。
- `git diff --check` 通过。仅对新建 `overlay.rs` 运行 rustfmt，没有重排共享工作区的其它既有格式差异。
- `pnpm debug:win` 通过（Windows cargo 2 分 50 秒）；脚本内 `pnpm check:win` 确认 exe 新于 dist、4 个入口引用资源全部命中，RAW worker 新于 Rust 源码且包含 `raybend-worker-proto-v3`。产物：`C:\rb-target\raybend\debug\raybend-desktop.exe`。

## 环境与未验证部分

- WSL 普通 sandbox 命令在执行前报 bubblewrap 挂载错误（`/mnt/wslg/distro`）。通过获准的 `require_escalated` 本机命令完成操作，没有改 sandbox 配置。Pencil 首次截图遇到新布局尚未刷新，重新读取实际 bounds 并截图后正常，没有以空白截图作为通过依据。
- 尚待崔总在 Windows 真机确认：三类拖动的流畅度，旋转拉线可见性，图像/裁切框/对比分线跟随，多显示器/DPI，右栏滚动及中英文面板布局。未声称真实照片、色彩或 E2E 已验证。
- 保留仓内原有未提交改动；没有提交、推送、打 tag 或生成发布包。
