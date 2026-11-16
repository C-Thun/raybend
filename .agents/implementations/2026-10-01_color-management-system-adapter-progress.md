2026-10-01 01:50:08 CST

# 色彩管理系统 adapter 阶段实施

## 改动范围与涉及文件

- `crates/raybend/src/color/system.rs`：平台无关的 `ColorSystemAdapter`、订阅接缝和 `ColorEnvironment`；每窗口记录当前显示事实与代数，跨屏失效后旧的并发读取不能覆盖新结果。fake 覆盖状态变化、无效值、乱序与关闭窗口。
- `src-tauri/src/color_system.rs`、`src-tauri/src/lib.rs`、`src-tauri/Cargo.toml`：Windows adapter 以物理窗口位置找实际显示器，再由 active display topology 取得目标，区分 Advanced Color 系统管理和传统 ICC；旧系统/驱动明确不支持查询时回退传统 ICC，其它错误保持不可用。系统入口只打开 Windows 显示设置，不写系统配置。
- `src/api/color.ts`、`src/shell/GlobalSettingsDialog.tsx`、中英语言包：统一设置弹窗显示真实的 ICC、系统管理、sRGB 回退、不可用、检测中状态；可重新检测或打开系统设置。跨屏移动和恢复焦点时重新读取；路径不被称作“已校准”，渲染未接线时也不声称显示转换已生效。

## 关键决策与命令

系统查询与打开设置只经 adapter 和 `src/api/`；照片/XMP/缓存不持久化本机显示器状态。传统 ICC 路径与 Advanced Color 路径互斥，避免双重转换。当前使用整个窗口的物理矩形做诊断；后续真实呈现应采用照片视口矩形。系统设置和手动刷新只在设置页上下文出现，不占命令面板的全局热键；打开全局设置仍通过已有 `settings.open`（`Ctrl+,`）。

## 验证

`cargo check --workspace`、Windows `pnpm debug:win` 构建与产物核对通过（包含静态 Little CMS 和 worker v6）；`pnpm typecheck`、`pnpm lint:i18n`、`pnpm lint:arch` 通过，系统 adapter fake 单测通过。真实多屏、DPI、ICC 切换、ACM/HDR 和 GUI 状态需要崔总真机 E2E。

## 遗留

系统 profile/ACM/HDR 的原生事件订阅、跨屏时对照片视口的定位、ICC 文件内容校验、GPU 末端转换与呈现色彩空间协商尚未完成。当前系统状态只用于诊断，不参与照片实际呈现。Win10/11 的不同驱动组合需真机样本核对，失败状态保持可见而不猜测。
