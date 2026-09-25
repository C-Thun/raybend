# M3-W5 裁切、旋转、对比视口交互
完成时间：2026-09-25 20:54:06 CST

## 范围与文件

- `crates/raybend/src/develop/geometry.rs`、`render/viewport.rs`：原图坐标归一化的成片框、旋转后的有像素边界、八把手命中与拖动、固定比例、水平内接框、CSS/DPR 转换和对比分线。前端只送窗口 CSS 指针事实。
- `src-tauri/src/editor.rs`、`crates/raybend/src/render/gpu.rs`：工具草稿驻留渲染线程；进入工具时显示原图，确认才写回成片几何，取消直接丢草稿。GPU 在同一视口/scissor 中对分画参考帧与当前结果；分线拖动由 Rust 裁定。配对 SOOC 位图由既有 `display::pixels` 入口解码，缺少时使用当前源的未调节参考帧；状态栏标出实际来源，可在当前支持的 SOOC/RAW 之间切换。
- `crates/raybend/src/store/migrations/catalog_0009_edit_geometry.sql`、`store/develop.rs`、`src-tauri/src/develop.rs`：几何随 latest 编辑栈保存，进入签名、空栈判定和撤销设置差异；schema 升级只走既有迁移框架。
- `crates/raybend/src/thumbnail/render.rs`、`develop/reference.rs`：编辑视口、缩略图、看图缓存和对比参考复用同一几何采样；先应用成片几何再做网格的 3:1 展示夹取，紧裁切预留更多源像素。缓存管线版本与签名同时升至 v11。
- `src/features/editor/viewport.tsx`、`ViewportOverlay.tsx`、`panels.tsx`、`src/workspaces/editor/EditorWorkspace.tsx`、共享 `PhotoStatusBar`：显示 4px 框、三分线、八把手、框外遮罩和分线；比例/角度控制、确认/取消、Esc、照片切换与状态栏来源选择接入会话。工具命令登记 C/R/B 默认键，检测了保留键与冲突。
- `src/api/` DTO、双语言文案和 `design/editor.md` 同步更新；沿用已有 `design/editor.pen`，没有另建一套画稿。

## 关键决定

- 水平成片框需在已旋转画布上保持水平，故由 Rust 返回洞口内 CSS 矩形，借现有 `ViewportOverlay.screenChildren` 展示；图像坐标的未来蒙版继续使用原有仿射矩阵。控件层不复制 zoom、pan 或 DPR 计算。
- SOOC/RAW 是当前已有的两个参考源。其他 issue 的选择入口复用同一状态栏菜单，待 W6 建立多 issue 数据模型后扩充；此波不造临时 issue 存储。
- 工具模式只编辑内存草稿，松手不写库，确认才写入已有编辑栈与预览刷新路径。旋转拉线的结果通过渲染状态修订号同步回角度控件。

## 已验证（Agent 冒烟与单元测试）

- `cargo check -p raybend -p raybend-desktop` 通过。
- `cargo test -p raybend --lib`：1057 通过、2 项既有手动测试忽略；`cargo test -p raybend-desktop --lib`：81 通过、1 项真实 RAW 手动测试忽略。覆盖几何边界/八把手/DPR、成片采样、迁移与落库、缩略图顺序与源分辨率、SOOC 像素缓存、撤销差异及 IPC 契约。
- `pnpm typecheck`、`pnpm test`（933 通过）、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm build` 通过；`git diff --check` 通过。
- `cargo fmt --all -- --check` 未通过：共享工作区里已有约 9019 行格式差异，涉及本波以外的 examples 等并行改动；未对全仓执行重排。

## 未经人类验证 / 遗留

- Windows 真机 GUI/E2E 尚待人类确认：旋转后裁切框沿斜画布边界、八把手/比例/拉线交互、不同 DPI 与跨屏、SOOC/RAW 对分色彩及纹理对齐、重启后缩略图与看图一致、撤销重做观感。Agent 没有把编译或进程启动视为这些项目已通过。
- SOOC 参照目前取 Screen 档，极限放大时不作为原片像素级锐度依据。完整 issue 列表及切换需要 W6 的多 issue 模型。
- 工作区包含其他会话的 W3/W4 等大量未提交改动；本次没有清理或提交它们，也没有推送、打 tag 或生成发布物。
