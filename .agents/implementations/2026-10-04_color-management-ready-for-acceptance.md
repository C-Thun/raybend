2026-10-04 15:48:01 +08:00

# 色彩管理首版界面与状态收尾

承接核心攻关记录 §6，范围为 `specs/cm-w3-status.md`。沿用原有色彩算法和 UI 框架；代码未提交。本轮首版开发、单元/组件/UI 冒烟与 Windows 配套产物核对全部完成，可交崔总统一实机验收。

## 改动与决策

- `src-tauri/src/color_status.rs` 输出结构化的实际呈现状态，明确 ICC 生效、系统 scRGB/sRGB、回退与不可用。携带显示目标、ICC 路径、SDR 白电平、准备代数与诊断。状态只在 renderer 安装转换并协商输出后产生，不拿系统请求的空间冒充实际 surface。`editor.rs` 不再把裸英文状态交给界面。
- 核心 `PreparedDisplay` 携带准备依据的原系统 snapshot，后台 watcher 的缓存命中也保留同一依据；只增加元数据，不改变矩阵、ICC 数值、像素、shader 或上传路径。设置的重新检测同时通知既有 native watcher 重新读盘并准备；全部 OS 能力继续通过 adapter。
- `lib/display-color.ts` 只持有类型、模式和当前目标一致性判断；`i18n/display-color.ts` 只持有文案映射，UI 原语只向下依赖。编辑面板和设置页共用 `DisplayColorStatus`，没有第二份显示判定或文案逻辑。两处现有组件均不能扩成这种诊断摘要，因此新增这个共享原语。
- 设置页上方说明编辑画布实际状态，下方明确系统检测事实；没有画布时显示未开启。设置页仅在打开且处于色彩页时每 500ms 顺序读一次原生状态，不重读 ICC/照片；关闭/切页取消定时器，迟到结果丢弃。跨屏、ICC 路径、ACM 或白电平不一致时隐藏旧成功结论并显示更新中。
- 显示转换的路径或失败细节可展开，Unicode/长路径换行。系统管理、回退和不可用都有中英文解释；修正“仍在接入”的过时文案。`settings.coming` 的现有正确文案保持不变。色彩面板补充缓存说明：打开面板/换屏/打样不重建缓存，应用输入更新受影响的照片预览。
- Pencil 先更新 `main.pen` 两主题和 `editor.pen` 色彩面板，再实现；同名说明同步。摘要条沿用原设计，系统事实是次要行；稿件已截图检查，新增文字 fit_content 的首次布局警告已复读修正，无剩余溢出。文件仍由崔总手动保存。

涉及文件：`crates/raybend/src/color/{display,system}.rs`，`src-tauri/src/{color_status,color_system,editor,lib,contract}.rs`，`src/api/{color,types,dto-contract.test}.ts` / `dto-contract.json`，`src/lib/display-color{,.test}.ts`，`src/i18n/display-color{,.test}.ts` / `zh-CN.ts` / `en-US.ts`，`src/components/ui/DisplayColorStatus.tsx`，`src/features/editor/panels.tsx`，`src/shell/GlobalSettingsDialog.tsx`，`scripts/check-color-status.mjs`，两份设计说明和当前规格/路线摘要。保留并行 AI/XMP/预设改动。

## 命令与 XMP 影响

没有新增独立操作。设置仍共用菜单、Ctrl+K、Mod+, 和 flowbar 齿轮入口；已有重新检测是当前页的维护按钮，不新增全局命令或占用热键。色彩面板/恢复自动/批量/打样等既有命令与无默认热键决定继续有效。

新增状态只属于本机显示会话，不进入照片色彩身份、编辑栈、定稿、预设、持久缓存或 XMP；不改变 `specs/xmp-sidecar.md` 的 schema 和输出字段。旧照片/issue 保持处理版本，显式应用输入才进入高精度路径。

## 验证

- 结构化状态测试覆盖实际协商输出而非系统期望值、ICC 目标/Unicode 超长路径、最大代数、明确回退/探测不可用/准备失败；Rust/TS 共用 DTO 键表。首次完整测试发现新增 DTO 漏登到契约覆盖清单，已补登记；没有放宽测试。
- `pnpm typecheck`、`pnpm test` 1207/1207 通过；colors/arch/i18n 通过。新增逻辑在最初分层检查中被识别为 UI/API 反向依赖，已按既有架构分成纯模型与纯文案映射，最终没有新增例外。
- `pnpm exec node scripts/check-color-status.mjs http://127.0.0.1:1432/`：22 项真实 Solid 组件冒烟通过，包括中英切换、同一 DOM 的状态更新、诊断内容与长路径、无画布状态和设置关闭/齿轮反馈；使用既有 CDP 工装与合成状态，不操作真实照片。
- `CDP_PORT=9533 pnpm smoke:ui http://127.0.0.1:1432/` 通过，problems=[]。
- `cargo test --workspace` 最终通过：核心库 1439 passed / 12 ignored，worker 集成 7 passed，桌面库 112 passed / 1 ignored，主程序 2 passed，文档 2 passed；合计 1562 passed / 13 ignored，零失败。`cargo check --workspace` 通过。日志 `/tmp/raybend-color-status-workspace-tests-final.log` 与 `-workspace-check.log`。

- `pnpm debug:win` 完成前端构建及 Windows 主程序/worker 配套构建，`check:win` 核对通过：4 个前端引用资源全部命中，worker 协议 v7。主程序 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`（2026-10-04T07:45:00.659Z），worker 同目录（2026-10-04T07:42:34.337Z）。日志 `/tmp/raybend-color-status-debug-win.log`。没有使用 release 或发版操作。
- 最终 `git diff --check` 通过。本轮没有重测算法性能或重写已通过的色彩数值路径；真实 ICC/DX12、60MP 有界后台上传及真实 RAW 数值证据继续见核心攻关记录。进程/组件冒烟不等于屏幕颜色与 GUI 观感已通过。

## 统一实机验收路径

这部分描述怎么评估首版，验收结论尚未填写；不另建待办文件。

1. 从菜单、Mod+,、flowbar 齿轮打开同一设置窗，检查深浅主题、紧凑/宽松、图标尺寸、激活色、滚动与关闭。进入/退出编辑后，设置中的画布状态应分别显示实际方式/未开启；诊断长路径应可读。
2. 导入 ICC/ICM，尝试重复、无效文件与隐藏/恢复；选择器共用真实资源。保存默认规则后重开确认，旧已编辑照片与定稿不应跟着默认变更而改变。
3. 对 RAW、有嵌入 ICC 的位图、无配置位图应用输入并撤销；尝试恢复自动和批量指定，核对锁定/源变化/不相容跳过提示及整批撤销。旧定稿应保留原样。
4. 在实际屏幕/多屏/DPI/ACM 环境里查看并移动窗口：画布显示方式应跟随当前屏幕，设置的系统事实和实际方式可能不同但各自必须准确。重新检测应重新准备当前配置；失败应明确回退。真实颜色、暗部/高光及广色域观感需要在校准环境判断。
5. 使用 sRGB/P3/Adobe RGB 开关软打样和色域警告，检查关闭恢复与提示；复杂 CLUT 打样不得假称可用。打开面板、换屏或打样不应引发照片缓存重建。
6. 核对导出目标与文件 ICC/标准标记、预设可选色彩组和 XMP 色彩身份；AVIF 首版目标为 sRGB。在真实大图上检查切图、缩放、平移、滑杆、对比与后台上传时的交互，整窗尾延迟不能由离屏数字替代。

首版边界：复杂 RGB 显示 ICC 为受数值验证的支持子集；复杂 CLUT 打样、完整 HDR 照片/打印/CMYK、DCP 全面兼容和 macOS/Linux 实装仍属后续范围。
